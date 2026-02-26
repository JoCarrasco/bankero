mod cli;
mod config;
mod db;
mod domain;
mod sync;
mod upgrade;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use clap::Parser;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::BTreeMap;
use std::io::{self, Write};
use uuid::Uuid;

use crate::cli::{
    BudgetCmd, Cli, Command, PiggyCmd, ProjectCmd, RateCommand, WsCmd, parse_provider_opt,
};
use crate::config::{AppConfig, app_paths, load_or_init_config, now_utc, write_config};
use crate::db::Db;
use crate::domain::{
    BasisContext, EventPayload, Posting, ProviderToken, RateContext, StoredEvent, parse_basis_arg,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let paths = app_paths(cli.home.clone())?;
    let (mut cfg, cfg_path) = load_or_init_config(&paths)?;

    match cli.command {
        Command::Login(args) => {
            crate::sync::handle_login(args, &mut cfg, &cfg_path)?;
            Ok(())
        }
        Command::Ws(args) => {
            handle_ws(args.cmd, &paths, &mut cfg, &cfg_path)?;
            Ok(())
        }
        Command::Project(args) => {
            handle_project(args.cmd, &paths, &mut cfg, &cfg_path)?;
            Ok(())
        }
        Command::Upgrade(args) => crate::upgrade::handle_upgrade(args),
        cmd => {
            let (db, db_path) = Db::open(&paths, &cfg.current_workspace)?;

            match cmd {
                Command::Deposit(args) => {
                    let confirm = args.common.confirm;
                    let event_id = Uuid::new_v4();
                    let payload = build_deposit_event(
                        &cfg,
                        "deposit",
                        event_id,
                        args.amount,
                        args.commodity,
                        args.from,
                        args.to,
                        None,
                        args.common,
                    )?;
                    maybe_confirm_and_insert(&db, &cfg, event_id, &payload, confirm)?;
                    println!("Wrote event {event_id} to {}", db_path.display());
                }
                Command::Move(args) => {
                    let (to_amount, to_commodity, provider) = parse_move_tail(&args.tail)?;
                    let confirm = args.common.confirm;
                    let event_id = Uuid::new_v4();

                    // If the user supplied only a destination commodity + provider, compute the quote amount.
                    let (to_amount, provider) = match (to_amount, to_commodity.as_ref(), provider) {
                        (None, Some(to_commodity), Some(mut provider)) => {
                            let amount = parse_decimal(args.amount.clone(), "amount")?;
                            let effective_at =
                                parse_rfc3339_or_now(args.common.effective_at.as_deref())?;
                            let as_of = parse_as_of(&args.common, effective_at)?;

                            let base = args.commodity.to_ascii_uppercase();
                            let quote = to_commodity.to_ascii_uppercase();

                            let rate = if let Some(r) = provider.override_rate {
                                r
                            } else {
                                let Some((_found_as_of, r)) =
                                    db.get_rate_as_of(&provider.provider, &base, &quote, as_of)?
                                else {
                                    return Err(anyhow!(
                                        "No stored rate for @{} {} per {} at or before {}. Set one with: bankero rate set @{} {} {} <rate> --as-of <rfc3339>",
                                        provider.provider,
                                        quote,
                                        base,
                                        as_of.to_rfc3339(),
                                        provider.provider,
                                        base,
                                        quote,
                                    ));
                                };
                                r
                            };

                            provider.override_rate = Some(rate);
                            let computed_to_amount = amount * rate;
                            (Some(computed_to_amount), Some(provider))
                        }
                        (to_amount, _, provider) => (to_amount, provider),
                    };

                    let payload = build_move_event(
                        &cfg,
                        event_id,
                        args.amount,
                        args.commodity,
                        args.from,
                        args.to,
                        provider,
                        to_amount,
                        to_commodity,
                        args.common,
                    )?;
                    maybe_confirm_and_insert(&db, &cfg, event_id, &payload, confirm)?;
                    println!("Wrote event {event_id} to {}", db_path.display());
                }
                Command::Buy(args) => {
                    let provider = parse_provider_opt(&args.provider);
                    let confirm = args.common.confirm;
                    let event_id = Uuid::new_v4();

                    let (payee, amount, commodity) = if let Some(commodity) = args.commodity {
                        (
                            Some(args.payee_or_amount),
                            args.amount_or_commodity,
                            commodity,
                        )
                    } else {
                        (None, args.payee_or_amount, args.amount_or_commodity)
                    };

                    let payload = build_buy_event(
                        &cfg,
                        event_id,
                        payee,
                        amount,
                        commodity,
                        args.from,
                        args.to_splits,
                        provider,
                        args.common,
                    )?;
                    maybe_confirm_and_insert(&db, &cfg, event_id, &payload, confirm)?;
                    println!("Wrote event {event_id} to {}", db_path.display());
                }
                Command::Sell(args) => {
                    let provider = parse_provider_opt(&args.provider);
                    let confirm = args.common.confirm;
                    let event_id = Uuid::new_v4();
                    let payload = build_sell_event(
                        &cfg,
                        event_id,
                        args.amount,
                        args.commodity,
                        args.from,
                        args.to,
                        args.to_amount,
                        args.to_commodity,
                        provider,
                        args.common,
                    )?;
                    maybe_confirm_and_insert(&db, &cfg, event_id, &payload, confirm)?;
                    println!("Wrote event {event_id} to {}", db_path.display());
                }
                Command::Tag(args) => {
                    let confirm = args.common.confirm;
                    let event_id = Uuid::new_v4();
                    let payload =
                        build_tag_event(&cfg, event_id, args.target, args.set_basis, args.common)?;
                    maybe_confirm_and_insert(&db, &cfg, event_id, &payload, confirm)?;
                    println!("Wrote event {event_id} to {}", db_path.display());
                }
                Command::Balance(args) => {
                    let events = db.list_events()?;
                    print_balance(&db, &events, args.account.as_deref(), args.month.as_deref())?;
                }
                Command::Report(args) => {
                    let events = db.list_events()?;
                    let filtered = filter_events(&events, &args)?;
                    print_report(&filtered);
                }
                Command::Rate(args) => {
                    handle_rate(&db, args.command)?;
                }
                Command::Budget(args) => {
                    handle_budget(&db, args.cmd)?;
                }
                Command::Piggy(args) => {
                    handle_piggy(&db, args.cmd)?;
                }
                Command::Sync(args) => {
                    crate::sync::handle_sync(&db, args, &mut cfg, &cfg_path)?;
                }
                Command::Task(_) | Command::Workflow(_) => {
                    eprintln!("This command is a stub for later milestones.");
                }
                Command::Ws(_) | Command::Project(_) | Command::Upgrade(_) | Command::Login(_) => {
                    unreachable!()
                }
            }

            Ok(())
        }
    }
}

fn normalize_provider(raw: &str) -> String {
    raw.trim().trim_start_matches('@').to_string()
}

fn current_month_yyyy_mm(now: DateTime<Utc>) -> String {
    format!("{:04}-{:02}", now.year(), now.month())
}

fn handle_budget(db: &Db, cmd: BudgetCmd) -> Result<()> {
    match cmd {
        BudgetCmd::Create {
            name,
            amount,
            commodity,
            month,
            category,
            account,
            extra,
        } => {
            if let Some(m) = month.as_deref() {
                let _ = parse_month_range(m)?;
            }

            let amount = parse_decimal(amount, "amount")?;
            let commodity = commodity.to_ascii_uppercase();

            let provider = parse_budget_provider(&extra)?;

            let budget = crate::db::StoredBudget {
                id: Uuid::new_v4(),
                name: name.clone(),
                amount,
                commodity: commodity.clone(),
                month,
                category,
                account,
                provider,
                auto_reserve_from: None,
                auto_reserve_until_amount: None,
                created_at: now_utc(),
            };

            db.insert_budget(&budget)?;
            println!("Created budget '{}' {} {}.", name, budget.amount, commodity);
            Ok(())
        }
        BudgetCmd::Update {
            name,
            auto_reserve_from,
            until,
            clear_auto_reserve,
        } => {
            let Some(budget) = db.get_budget_by_name(&name)? else {
                return Err(anyhow!("No such budget: '{name}'"));
            };

            if clear_auto_reserve {
                let changed = db.set_budget_auto_reserve(&name, None, None)?;
                if changed == 0 {
                    return Err(anyhow!("No such budget: '{name}'"));
                }
                println!("Cleared auto-reserve for budget '{name}'.");
                return Ok(());
            }

            let from_prefix = auto_reserve_from
                .as_deref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let until_amount = match until {
                None => None,
                Some(parts) => {
                    if parts.len() != 2 {
                        return Err(anyhow!("--until expects: <amount> <commodity>"));
                    }
                    let amount = parse_decimal(parts[0].clone(), "until amount")?;
                    let comm = parts[1].to_ascii_uppercase();
                    let budget_comm = budget.commodity.to_ascii_uppercase();
                    if comm != budget_comm {
                        return Err(anyhow!(
                            "--until commodity must match budget commodity ({} != {})",
                            comm,
                            budget_comm
                        ));
                    }
                    Some(amount)
                }
            };

            if from_prefix.is_some() {
                if budget.account.is_none() {
                    return Err(anyhow!(
                        "Auto-reserve requires the budget to be scoped to an account. Create the budget with: --account <account>"
                    ));
                }
                if budget.category.is_none() {
                    return Err(anyhow!(
                        "Auto-reserve requires the budget to have a category. Create the budget with: --category <category>"
                    ));
                }
            }

            let changed =
                db.set_budget_auto_reserve(&name, from_prefix.as_deref(), until_amount)?;
            if changed == 0 {
                return Err(anyhow!("No such budget: '{name}'"));
            }

            if let Some(from) = from_prefix {
                let until_display = until_amount
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "(none)".to_string());
                println!(
                    "Updated budget '{name}': auto-reserve from '{from}', until {until_display} {}.",
                    budget.commodity
                );
            } else {
                println!("Updated budget '{name}'.");
            }

            Ok(())
        }
        BudgetCmd::Report { month } => {
            let month = month.unwrap_or_else(|| current_month_yyyy_mm(now_utc()));
            let (start, end) = parse_month_range(&month)?;

            let budgets = db.list_budgets()?;
            let mut budgets: Vec<_> = budgets
                .into_iter()
                .filter(|b| match b.month.as_deref() {
                    None => true,
                    Some(m) => m == month,
                })
                .collect();
            budgets.sort_by(|a, b| a.name.cmp(&b.name));

            if budgets.is_empty() {
                println!("(no budgets)");
                return Ok(());
            }

            let events = db.list_events()?;
            println!("month\tname\tcommodity\tbudget\tactual\tremaining");
            for b in budgets {
                let actual = compute_budget_actual(&events, start, end, &b);
                let remaining = b.amount - actual;
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    month, b.name, b.commodity, b.amount, actual, remaining
                );
            }
            Ok(())
        }
    }
}

fn handle_piggy(db: &Db, cmd: PiggyCmd) -> Result<()> {
    match cmd {
        PiggyCmd::Create {
            name,
            amount,
            commodity,
            from,
        } => {
            let target_amount = parse_decimal(amount, "amount")?;
            if target_amount <= Decimal::ZERO {
                return Err(anyhow!("Piggy target amount must be > 0"));
            }

            let piggy = crate::db::StoredPiggy {
                id: Uuid::new_v4(),
                name: name.clone(),
                target_amount,
                commodity: commodity.to_ascii_uppercase(),
                from_account: from,
                created_at: now_utc(),
            };

            db.insert_piggy(&piggy)
                .with_context(|| format!("Failed to create piggy '{name}'"))?;
            println!(
                "Created piggy '{}' target {} {} (from {}).",
                piggy.name, piggy.target_amount, piggy.commodity, piggy.from_account
            );
            Ok(())
        }
        PiggyCmd::List => {
            let piggies = db.list_piggies()?;
            if piggies.is_empty() {
                println!("(no piggies)");
                return Ok(());
            }

            println!("name\tcommodity\ttarget\tfunded\tpercent\tfrom");
            for p in piggies {
                let funded = db.piggy_funded_total(p.id)?;
                let funded_capped = funded.min(p.target_amount);
                let percent = if p.target_amount > Decimal::ZERO {
                    (funded_capped / p.target_amount) * Decimal::from(100u32)
                } else {
                    Decimal::ZERO
                };
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    p.name,
                    p.commodity,
                    p.target_amount,
                    funded,
                    percent.round_dp(2),
                    p.from_account
                );
            }
            Ok(())
        }
        PiggyCmd::Status { name } => {
            let Some(piggy) = db.get_piggy_by_name(&name)? else {
                return Err(anyhow!("No such piggy: '{name}'"));
            };

            let funded = db.piggy_funded_total(piggy.id)?;
            let funded_capped = funded.min(piggy.target_amount);
            let percent_f = if piggy.target_amount > Decimal::ZERO {
                (funded_capped / piggy.target_amount) * Decimal::from(100u32)
            } else {
                Decimal::ZERO
            };
            let percent_i = percent_f.round_dp(0).to_i32().unwrap_or(0).clamp(0, 100);

            let bar_len = 10usize;
            let filled = ((percent_i as usize) * bar_len) / 100;
            let empty = bar_len.saturating_sub(filled);
            let bar = format!("[{}{}]", "=".repeat(filled), "-".repeat(empty));

            let remaining = (piggy.target_amount - funded).max(Decimal::ZERO);
            println!(
                "{} {}% ({} / {} {})",
                bar, percent_i, funded, piggy.target_amount, piggy.commodity
            );
            println!("remaining\t{}\t{}", piggy.commodity, remaining);
            println!("from\t{}", piggy.from_account);
            Ok(())
        }
        PiggyCmd::Fund {
            name,
            amount,
            commodity,
            effective_at,
        } => {
            let Some(piggy) = db.get_piggy_by_name(&name)? else {
                return Err(anyhow!("No such piggy: '{name}'"));
            };

            if let Some(comm) = commodity {
                let comm = comm.to_ascii_uppercase();
                if comm != piggy.commodity {
                    return Err(anyhow!(
                        "Piggy '{}' is in {} but fund was {}. Omit the commodity to use the piggy commodity.",
                        piggy.name,
                        piggy.commodity,
                        comm
                    ));
                }
            }

            let amount = parse_decimal(amount, "amount")?;
            if amount <= Decimal::ZERO {
                return Err(anyhow!("Fund amount must be > 0"));
            }
            let effective_at = parse_rfc3339_or_now(effective_at.as_deref())?;

            let fund = crate::db::StoredPiggyFund {
                id: Uuid::new_v4(),
                piggy_id: piggy.id,
                amount,
                effective_at,
                created_at: now_utc(),
            };
            db.insert_piggy_fund(&fund)?;
            println!(
                "Funded piggy '{}' {} {} (from {}).",
                piggy.name, fund.amount, piggy.commodity, piggy.from_account
            );
            Ok(())
        }
    }
}

fn parse_budget_provider(extra: &[String]) -> Result<Option<String>> {
    let mut provider: Option<String> = None;
    for token in extra {
        if let Some(p) = crate::domain::parse_provider_token(token) {
            if provider.is_some() {
                return Err(anyhow!(
                    "budget create accepts at most one provider token (e.g. @binance)"
                ));
            }
            if p.override_rate.is_some() {
                return Err(anyhow!(
                    "budget provider token must not include an override rate (use @provider, not @provider:rate)"
                ));
            }
            provider = Some(normalize_provider(&p.provider));
        } else {
            return Err(anyhow!(
                "Unrecognized extra argument for budget create: {token}"
            ));
        }
    }
    Ok(provider)
}

fn compute_budget_actual(
    events: &[StoredEvent],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    budget: &crate::db::StoredBudget,
) -> Decimal {
    let mut total = Decimal::ZERO;
    let budget_comm = budget.commodity.to_ascii_uppercase();

    for e in events {
        if e.action != "buy" {
            continue;
        }
        if e.effective_at < start || e.effective_at > end {
            continue;
        }
        if let Some(cat) = &budget.category {
            if e.payload.category.as_deref() != Some(cat.as_str()) {
                continue;
            }
        }

        for p in &e.payload.postings {
            if p.amount >= Decimal::ZERO {
                continue;
            }
            if p.commodity.to_ascii_uppercase() != budget_comm {
                continue;
            }
            if let Some(acct) = &budget.account {
                if !p.account.starts_with(acct) {
                    continue;
                }
            }
            total += -p.amount;
        }
    }

    total
}

fn compute_budget_funded(
    events: &[StoredEvent],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    to_account_prefix: &str,
    commodity: &str,
    from_account_prefix: &str,
) -> Decimal {
    let mut total = Decimal::ZERO;
    let comm = commodity.to_ascii_uppercase();

    for e in events {
        if e.effective_at < start || e.effective_at > end {
            continue;
        }

        // Identify credits to the destination account.
        let mut credit_sum = Decimal::ZERO;
        for p in &e.payload.postings {
            if p.amount <= Decimal::ZERO {
                continue;
            }
            if p.commodity.to_ascii_uppercase() != comm {
                continue;
            }
            if !p.account.starts_with(to_account_prefix) {
                continue;
            }
            credit_sum += p.amount;
        }

        if credit_sum.is_zero() {
            continue;
        }

        // Ensure the event came from the desired source account prefix.
        let from_match = e
            .payload
            .postings
            .iter()
            .any(|p| p.amount < Decimal::ZERO && p.account.starts_with(from_account_prefix));

        if !from_match {
            continue;
        }

        total += credit_sum;
    }

    total
}

fn handle_rate(db: &Db, cmd: RateCommand) -> Result<()> {
    match cmd {
        RateCommand::Set(args) => {
            let provider = normalize_provider(&args.provider);
            let base = args.base.to_ascii_uppercase();
            let quote = args.quote.to_ascii_uppercase();
            let as_of = parse_rfc3339_or_now(args.as_of.as_deref())?;
            db.set_rate(&provider, &base, &quote, as_of, args.rate)?;
            println!(
                "Set rate @{} {} per {} = {} (as of {}).",
                provider,
                quote,
                base,
                args.rate,
                as_of.to_rfc3339()
            );
            Ok(())
        }
        RateCommand::Get(args) => {
            let provider = normalize_provider(&args.provider);
            let base = args.base.to_ascii_uppercase();
            let quote = args.quote.to_ascii_uppercase();
            let as_of = parse_rfc3339_or_now(args.as_of.as_deref())?;
            let Some((found_as_of, rate)) = db.get_rate_as_of(&provider, &base, &quote, as_of)?
            else {
                return Err(anyhow!(
                    "No stored rate for @{} {} per {} at or before {}",
                    provider,
                    quote,
                    base,
                    as_of.to_rfc3339()
                ));
            };

            println!(
                "@{} {} per {} = {} (as of {}).",
                provider,
                quote,
                base,
                rate,
                found_as_of.to_rfc3339()
            );
            Ok(())
        }
        RateCommand::List(args) => {
            let provider = normalize_provider(&args.provider);
            let base = args.base.as_ref().map(|b| b.to_ascii_uppercase());
            let quote = args.quote.as_ref().map(|q| q.to_ascii_uppercase());

            match (base.as_deref(), quote.as_deref()) {
                (None, None) => {
                    let rows = db.list_latest_rates_for_provider(&provider, args.limit)?;
                    if rows.is_empty() {
                        println!("(no rates)");
                        return Ok(());
                    }

                    match args.format {
                        crate::cli::RateListFormat::Table => {
                            let mut table_rows = Vec::new();
                            for (b, q, as_of, rate) in rows {
                                table_rows.push(vec![b, q, as_of.to_rfc3339(), rate.to_string()]);
                            }
                            print_table(&["BASE", "QUOTE", "AS OF", "RATE"], &table_rows);
                        }
                        crate::cli::RateListFormat::Tsv => {
                            for (b, q, as_of, rate) in rows {
                                println!("{}\t{}\t{}\t{}", b, q, as_of.to_rfc3339(), rate);
                            }
                        }
                    }
                    Ok(())
                }
                (Some(base), None) => {
                    let rows = db.list_latest_rates_for_base(&provider, base, args.limit)?;
                    if rows.is_empty() {
                        println!("(no rates)");
                        return Ok(());
                    }

                    match args.format {
                        crate::cli::RateListFormat::Table => {
                            let mut table_rows = Vec::new();
                            for (b, q, as_of, rate) in rows {
                                table_rows.push(vec![b, q, as_of.to_rfc3339(), rate.to_string()]);
                            }
                            print_table(&["BASE", "QUOTE", "AS OF", "RATE"], &table_rows);
                        }
                        crate::cli::RateListFormat::Tsv => {
                            for (b, q, as_of, rate) in rows {
                                println!("{}\t{}\t{}\t{}", b, q, as_of.to_rfc3339(), rate);
                            }
                        }
                    }
                    Ok(())
                }
                (Some(base), Some(quote)) => {
                    let rows = db.list_rates(&provider, base, quote, args.limit)?;
                    if rows.is_empty() {
                        println!("(no rates)");
                        return Ok(());
                    }

                    match args.format {
                        crate::cli::RateListFormat::Table => {
                            let mut table_rows = Vec::new();
                            for (as_of, rate) in rows {
                                table_rows.push(vec![as_of.to_rfc3339(), rate.to_string()]);
                            }
                            print_table(&["AS OF", "RATE"], &table_rows);
                        }
                        crate::cli::RateListFormat::Tsv => {
                            for (as_of, rate) in rows {
                                println!("{}\t{}", as_of.to_rfc3339(), rate);
                            }
                        }
                    }
                    Ok(())
                }
                (None, Some(_)) => Err(anyhow!(
                    "Invalid arguments: quote provided without base. Usage: bankero rate list @provider [BASE] [QUOTE]"
                )),
            }
        }
    }
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if headers.is_empty() {
        println!("(no columns)");
        return;
    }

    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();

    for row in rows {
        for (i, cell) in row.iter().take(cols).enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    fn print_row(cells: &[String], widths: &[usize]) {
        print!("|");
        for (i, w) in widths.iter().enumerate() {
            let cell = cells.get(i).map(String::as_str).unwrap_or("");
            print!(" {:width$} |", cell, width = *w);
        }
        println!();
    }

    fn print_sep(widths: &[usize]) {
        print!("|");
        for w in widths {
            print!("{}|", "-".repeat(w + 2));
        }
        println!();
    }

    let header_cells: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    print_row(&header_cells, &widths);
    print_sep(&widths);
    for row in rows {
        print_row(row, &widths);
    }
}

fn handle_ws(
    cmd: WsCmd,
    paths: &crate::config::AppPaths,
    cfg: &mut AppConfig,
    cfg_path: &std::path::Path,
) -> Result<()> {
    match cmd {
        WsCmd::Check => {
            println!("You are currently in workspace: {}", cfg.current_workspace);
            println!("and the current project is: '{}'", cfg.current_project);
        }
        WsCmd::Add { name } => {
            // Creating a workspace is just creating its db directory.
            let _ = Db::open(paths, &name)?;
            println!("Added workspace: {name}");
        }
        WsCmd::Checkout { name } => {
            let _ = Db::open(paths, &name)?;
            cfg.current_workspace = name.clone();
            cfg.current_project = "default".to_string();
            write_config(cfg_path, cfg)?;
            println!("Checked out workspace: {name}");
        }
    }
    Ok(())
}

fn handle_project(
    cmd: ProjectCmd,
    paths: &crate::config::AppPaths,
    cfg: &mut AppConfig,
    cfg_path: &std::path::Path,
) -> Result<()> {
    // For now projects are simple names stored in config; later they'll be persisted per-workspace.
    let (db, _) = Db::open(paths, &cfg.current_workspace)?;
    match cmd {
        ProjectCmd::Add { name } => {
            // noop persistence for now; just validate the db is available.
            let _ = db;
            println!("Added project: {name}");
        }
        ProjectCmd::Checkout { name } => {
            let _ = db;
            cfg.current_project = name.clone();
            write_config(cfg_path, cfg)?;
            println!("Checked out project: {name}");
        }
        ProjectCmd::List => {
            let _ = db;
            println!("Current project: {}", cfg.current_project);
            println!("(Project list persistence not implemented yet)");
        }
    }
    Ok(())
}

fn parse_decimal(raw: String, field: &'static str) -> Result<Decimal> {
    raw.parse::<Decimal>()
        .with_context(|| format!("Invalid decimal for {field}: {raw}"))
}

fn parse_rfc3339_or_now(raw: Option<&str>) -> Result<DateTime<Utc>> {
    match raw {
        None => Ok(now_utc()),
        Some(s) => Ok(DateTime::parse_from_rfc3339(s)
            .with_context(|| format!("Invalid RFC3339 timestamp: {s}"))?
            .with_timezone(&Utc)),
    }
}

fn parse_move_tail(
    tail: &[String],
) -> Result<(Option<Decimal>, Option<String>, Option<ProviderToken>)> {
    match tail.len() {
        0 => Ok((None, None, None)),
        1 => {
            let maybe_provider = tail[0].as_str();
            let provider = crate::domain::parse_provider_token(maybe_provider).ok_or_else(|| {
                anyhow!(
                    "Invalid move tail. Expected @provider or @provider:rate, got: {maybe_provider}"
                )
            })?;
            Ok((None, None, Some(provider)))
        }
        2 => {
            // Either:
            // - explicit quote: <to_amount> <to_commodity>
            // - computed quote: <to_commodity> @provider[:rate]
            if let Ok(to_amount) = parse_decimal(tail[0].clone(), "to_amount") {
                let to_commodity = tail[1].clone();
                return Ok((Some(to_amount), Some(to_commodity), None));
            }

            let to_commodity = tail[0].clone();
            let provider = crate::domain::parse_provider_token(&tail[1]).ok_or_else(|| {
                anyhow!(
                    "Invalid move tail provider. Expected @provider or @provider:rate, got: {}",
                    tail[1]
                )
            })?;
            Ok((None, Some(to_commodity), Some(provider)))
        }
        3 => {
            let to_amount = parse_decimal(tail[0].clone(), "to_amount")?;
            let to_commodity = tail[1].clone();
            let provider = crate::domain::parse_provider_token(&tail[2]).ok_or_else(|| {
                anyhow!(
                    "Invalid move tail provider. Expected @provider or @provider:rate, got: {}",
                    tail[2]
                )
            })?;
            Ok((Some(to_amount), Some(to_commodity), Some(provider)))
        }
        _ => Err(anyhow!(
            "Invalid move tail. Expected at most 3 values: <to_amount> <to_commodity> [@provider[:rate]]"
        )),
    }
}

fn parse_as_of(
    common: &crate::cli::CommonEventFlags,
    effective_at: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    if let Some(as_of) = &common.as_of {
        let dt = DateTime::parse_from_rfc3339(as_of)
            .with_context(|| format!("Invalid RFC3339 timestamp for --as-of: {as_of}"))?
            .with_timezone(&Utc);
        return Ok(dt);
    }
    Ok(effective_at)
}

fn build_rate_context(
    provider: Option<ProviderToken>,
    as_of: DateTime<Utc>,
    base: Option<String>,
    quote: Option<String>,
) -> RateContext {
    RateContext {
        provider: provider.as_ref().map(|p| format!("@{}", p.provider)),
        override_rate: provider.and_then(|p| p.override_rate),
        base,
        quote,
        as_of,
    }
}

fn infer_ref_rate_pair(reference: &str, commodity: &str) -> (Option<String>, Option<String>) {
    if commodity == reference {
        (None, None)
    } else {
        (Some(reference.to_string()), Some(commodity.to_string()))
    }
}

fn build_deposit_event(
    cfg: &AppConfig,
    action: &str,
    _event_id: Uuid,
    amount_raw: String,
    commodity: String,
    from: String,
    to: String,
    provider: Option<ProviderToken>,
    common: crate::cli::CommonEventFlags,
) -> Result<EventPayload> {
    let amount = parse_decimal(amount_raw, "amount")?;
    let created_at = now_utc();
    let effective_at = parse_rfc3339_or_now(common.effective_at.as_deref())?;
    let as_of = parse_as_of(&common, effective_at)?;

    let postings = vec![
        Posting {
            account: from,
            commodity: commodity.clone(),
            amount: -amount,
        },
        Posting {
            account: to,
            commodity: commodity.clone(),
            amount,
        },
    ];

    let basis = common
        .basis
        .as_deref()
        .and_then(parse_basis_arg)
        .or_else(|| parse_fixed_basis(&common.basis));

    Ok(EventPayload {
        schema_version: 1,
        device_id: cfg.device_id,
        workspace: cfg.current_workspace.clone(),
        project: cfg.current_project.clone(),
        action: action.to_string(),
        created_at,
        effective_at,
        postings,
        tags: common.tags,
        category: common.category,
        note: common.note,
        rate_context: build_rate_context(provider, as_of, None, None),
        basis,
        metadata: serde_json::json!({"confirm": common.confirm}),
    })
}

fn build_move_event(
    cfg: &AppConfig,
    event_id: Uuid,
    amount_raw: String,
    commodity: String,
    from: String,
    to: String,
    provider: Option<ProviderToken>,
    to_amount: Option<Decimal>,
    to_commodity: Option<String>,
    common: crate::cli::CommonEventFlags,
) -> Result<EventPayload> {
    let amount = parse_decimal(amount_raw, "amount")?;
    let created_at = now_utc();
    let effective_at = parse_rfc3339_or_now(common.effective_at.as_deref())?;
    let as_of = parse_as_of(&common, effective_at)?;

    let (to_amount, to_commodity, inferred_rate) = match (to_amount, to_commodity) {
        (Some(to_amount), Some(c)) => {
            let inferred_rate = if amount.is_zero() {
                None
            } else {
                Some(to_amount / amount)
            };
            (Some(to_amount), Some(c), inferred_rate)
        }
        _ => (None, None, None),
    };

    let mut postings = vec![Posting {
        account: from,
        commodity: commodity.clone(),
        amount: -amount,
    }];

    if let Some((ta, tc)) = to_amount.zip(to_commodity) {
        postings.push(Posting {
            account: to,
            commodity: tc.clone(),
            amount: ta,
        });
        // If no explicit override rate was provided, derive it from provided amounts.
        let mut p = provider;
        if inferred_rate.is_some() {
            if let Some(pp) = &mut p {
                if pp.override_rate.is_none() {
                    pp.override_rate = inferred_rate;
                }
            } else {
                p = Some(ProviderToken {
                    provider: "derived".to_string(),
                    override_rate: inferred_rate,
                });
            }
        }

        let basis = common
            .basis
            .as_deref()
            .and_then(parse_basis_arg)
            .or_else(|| parse_fixed_basis(&common.basis));

        return Ok(EventPayload {
            schema_version: 1,
            device_id: cfg.device_id,
            workspace: cfg.current_workspace.clone(),
            project: cfg.current_project.clone(),
            action: "move".to_string(),
            created_at,
            effective_at,
            postings,
            tags: common.tags,
            category: common.category,
            note: common.note,
            rate_context: build_rate_context(p, as_of, Some(commodity), Some(tc)),
            basis,
            metadata: serde_json::json!({"event_id": event_id.to_string(), "confirm": common.confirm}),
        });
    }

    // Same currency move.
    postings.push(Posting {
        account: to,
        commodity: commodity.clone(),
        amount,
    });

    let basis = common
        .basis
        .as_deref()
        .and_then(parse_basis_arg)
        .or_else(|| parse_fixed_basis(&common.basis));

    Ok(EventPayload {
        schema_version: 1,
        device_id: cfg.device_id,
        workspace: cfg.current_workspace.clone(),
        project: cfg.current_project.clone(),
        action: "move".to_string(),
        created_at,
        effective_at,
        postings,
        tags: common.tags,
        category: common.category,
        note: common.note,
        rate_context: {
            let (base, quote) = match provider.as_ref() {
                None => (None, None),
                Some(_) => infer_ref_rate_pair(&cfg.reference_commodity, &commodity),
            };
            build_rate_context(provider, as_of, base, quote)
        },
        basis,
        metadata: serde_json::json!({"event_id": event_id.to_string(), "confirm": common.confirm}),
    })
}

fn build_buy_event(
    cfg: &AppConfig,
    event_id: Uuid,
    payee: Option<String>,
    amount_raw: String,
    commodity: String,
    from: String,
    to_splits: Vec<String>,
    provider: Option<ProviderToken>,
    common: crate::cli::CommonEventFlags,
) -> Result<EventPayload> {
    let payee_for_metadata = payee.clone();
    let amount = parse_decimal(amount_raw, "amount")?;
    let created_at = now_utc();
    let effective_at = parse_rfc3339_or_now(common.effective_at.as_deref())?;
    let as_of = parse_as_of(&common, effective_at)?;

    let mut postings = vec![Posting {
        account: from,
        commodity: commodity.clone(),
        amount: -amount,
    }];

    if to_splits.is_empty() {
        let payee = payee.ok_or_else(|| {
            anyhow!("buy requires either a payee/target (3-arg form) or at least one --to split (2-arg form)")
        })?;
        postings.push(Posting {
            account: payee,
            commodity: commodity.clone(),
            amount,
        });
    } else {
        let mut sum = Decimal::ZERO;
        for split in to_splits {
            let (account, split_amount) = parse_split_to(&split, &commodity)?;
            sum += split_amount;
            postings.push(Posting {
                account,
                commodity: commodity.clone(),
                amount: split_amount,
            });
        }
        if sum != amount {
            return Err(anyhow!(
                "Split amounts must sum to the buy amount ({} != {})",
                sum,
                amount
            ));
        }
    }

    let basis = common
        .basis
        .as_deref()
        .and_then(parse_basis_arg)
        .or_else(|| parse_fixed_basis(&common.basis));

    Ok(EventPayload {
        schema_version: 1,
        device_id: cfg.device_id,
        workspace: cfg.current_workspace.clone(),
        project: cfg.current_project.clone(),
        action: "buy".to_string(),
        created_at,
        effective_at,
        postings,
        tags: common.tags,
        category: common.category,
        note: common.note,
        rate_context: {
            let (base, quote) = match provider.as_ref() {
                None => (None, None),
                Some(_) => infer_ref_rate_pair(&cfg.reference_commodity, &commodity),
            };
            build_rate_context(provider, as_of, base, quote)
        },
        basis,
        metadata: serde_json::json!({
            "event_id": event_id.to_string(),
            "confirm": common.confirm,
            "payee": payee_for_metadata,
        }),
    })
}

fn build_sell_event(
    cfg: &AppConfig,
    event_id: Uuid,
    amount_raw: String,
    commodity: String,
    from: Option<String>,
    to: String,
    to_amount: Decimal,
    to_commodity: String,
    provider: Option<ProviderToken>,
    common: crate::cli::CommonEventFlags,
) -> Result<EventPayload> {
    let amount = parse_decimal(amount_raw, "amount")?;
    let created_at = now_utc();
    let effective_at = parse_rfc3339_or_now(common.effective_at.as_deref())?;
    let as_of = parse_as_of(&common, effective_at)?;

    let from_account = from.unwrap_or_else(|| format!("assets:{}", commodity.to_ascii_lowercase()));

    let inferred_rate = if amount.is_zero() {
        None
    } else {
        Some(to_amount / amount)
    };

    let mut p = provider;
    if inferred_rate.is_some() {
        if let Some(pp) = &mut p {
            if pp.override_rate.is_none() {
                pp.override_rate = inferred_rate;
            }
        } else {
            p = Some(ProviderToken {
                provider: "derived".to_string(),
                override_rate: inferred_rate,
            });
        }
    }

    let postings = vec![
        Posting {
            account: from_account,
            commodity: commodity.clone(),
            amount: -amount,
        },
        Posting {
            account: to,
            commodity: to_commodity.clone(),
            amount: to_amount,
        },
    ];

    let basis = common
        .basis
        .as_deref()
        .and_then(parse_basis_arg)
        .or_else(|| parse_fixed_basis(&common.basis));

    Ok(EventPayload {
        schema_version: 1,
        device_id: cfg.device_id,
        workspace: cfg.current_workspace.clone(),
        project: cfg.current_project.clone(),
        action: "sell".to_string(),
        created_at,
        effective_at,
        postings,
        tags: common.tags,
        category: common.category,
        note: common.note,
        rate_context: build_rate_context(p, as_of, Some(commodity), Some(to_commodity.clone())),
        basis,
        metadata: serde_json::json!({"event_id": event_id.to_string(), "confirm": common.confirm}),
    })
}

fn build_tag_event(
    cfg: &AppConfig,
    event_id: Uuid,
    target: String,
    set_basis: Option<String>,
    common: crate::cli::CommonEventFlags,
) -> Result<EventPayload> {
    let created_at = now_utc();
    let effective_at = parse_rfc3339_or_now(common.effective_at.as_deref())?;
    let as_of = parse_as_of(&common, effective_at)?;

    let basis = set_basis
        .as_deref()
        .and_then(parse_basis_arg)
        .or_else(|| parse_fixed_basis(&set_basis));

    Ok(EventPayload {
        schema_version: 1,
        device_id: cfg.device_id,
        workspace: cfg.current_workspace.clone(),
        project: cfg.current_project.clone(),
        action: "tag".to_string(),
        created_at,
        effective_at,
        postings: vec![],
        tags: common.tags,
        category: common.category,
        note: common.note,
        rate_context: RateContext {
            provider: None,
            override_rate: None,
            base: None,
            quote: None,
            as_of,
        },
        basis,
        metadata: serde_json::json!({"target": target, "event_id": event_id.to_string(), "confirm": common.confirm}),
    })
}

fn parse_fixed_basis(raw: &Option<String>) -> Option<BasisContext> {
    let raw = raw.as_deref()?;
    if raw.trim().starts_with('@') {
        return None;
    }
    let mut parts = raw.split_whitespace();
    let a = parts.next()?;
    let c = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let amount = a.parse::<Decimal>().ok()?;
    Some(BasisContext::Fixed {
        amount,
        commodity: c.to_string(),
    })
}

// parse_cross_currency_tail removed (explicit positionals used instead)

fn parse_split_to(raw: &str, commodity: &str) -> Result<(String, Decimal)> {
    // Split format: <account>:<amount>
    let (account, amount_raw) = raw
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("Invalid --to split '{raw}'. Expected <account>:<amount>"))?;
    let amount = amount_raw
        .parse::<Decimal>()
        .with_context(|| format!("Invalid split amount in '{raw}'"))?;
    if account.is_empty() {
        return Err(anyhow!("Invalid --to split '{raw}': empty account"));
    }
    let _ = commodity;
    Ok((account.to_string(), amount))
}

fn maybe_confirm_and_insert(
    db: &Db,
    cfg: &AppConfig,
    event_id: Uuid,
    payload: &EventPayload,
    confirm: bool,
) -> Result<()> {
    let mut payload = payload.clone();

    // Deterministic provider resolution (offline): if a provider is set but no override rate
    // exists, in confirm mode we resolve it from the local rate store.
    let provider_display = payload.rate_context.provider.clone();
    if confirm
        && provider_display.is_some()
        && payload.rate_context.override_rate.is_none()
        && payload.rate_context.base.is_some()
        && payload.rate_context.quote.is_some()
    {
        let provider_display = provider_display
            .clone()
            .unwrap_or_else(|| "@provider".to_string());
        let provider = normalize_provider(&provider_display);
        let base = payload
            .rate_context
            .base
            .clone()
            .unwrap_or_else(|| "base".to_string())
            .to_ascii_uppercase();
        let quote = payload
            .rate_context
            .quote
            .clone()
            .unwrap_or_else(|| "quote".to_string())
            .to_ascii_uppercase();

        let as_of = payload.rate_context.as_of;
        let Some((found_as_of, rate)) = db.get_rate_as_of(&provider, &base, &quote, as_of)? else {
            return Err(anyhow!(
                "No stored rate for {} ({} per {}) at or before {}. Set one with: bankero rate set {} {} {} <rate> --as-of <rfc3339>\nOr pass an explicit override like {}:<rate>.",
                provider_display,
                quote,
                base,
                as_of.to_rfc3339(),
                provider_display,
                base,
                quote,
                provider_display,
            ));
        };

        payload.rate_context.override_rate = Some(rate);
        payload.metadata["rate_resolved_as_of"] =
            serde_json::Value::String(found_as_of.to_rfc3339());
        eprintln!(
            "Using {} rate {} (as of {}).",
            provider_display,
            rate,
            found_as_of.to_rfc3339()
        );
    }

    if !confirm {
        db.insert_event(event_id, &payload)?;
        return Ok(());
    }

    // Deterministic basis computation: if a provider-based basis is requested,
    // compute a fixed basis amount in the reference commodity using the local rate store.
    if let Some(BasisContext::Provider { provider }) = payload.basis.clone() {
        let provider_display = provider;
        let provider = normalize_provider(&provider_display);

        let Some((from_amount, from_commodity)) = primary_outgoing_amount(&payload.postings) else {
            return Err(anyhow!(
                "Cannot compute basis for {}: no outgoing posting found",
                provider_display
            ));
        };

        let as_of = payload.rate_context.as_of;
        let to_commodity = cfg.reference_commodity.to_ascii_uppercase();
        let from_commodity = from_commodity.to_ascii_uppercase();

        let (basis_amount, rate_used, inverted, rate_as_of) = resolve_and_convert(
            db,
            &provider,
            &from_commodity,
            &to_commodity,
            as_of,
            from_amount,
        )
        .with_context(|| format!("Failed to compute basis via {provider_display}"))?;

        payload.basis = Some(BasisContext::Fixed {
            amount: basis_amount,
            commodity: to_commodity.clone(),
        });
        payload.metadata["basis_provider"] = serde_json::Value::String(provider_display.clone());
        payload.metadata["basis_rate_used"] = serde_json::Value::String(rate_used.to_string());
        payload.metadata["basis_rate_inverted"] = serde_json::Value::Bool(inverted);
        payload.metadata["basis_rate_as_of"] = serde_json::Value::String(rate_as_of.to_rfc3339());
        payload.metadata["basis_from_amount"] = serde_json::Value::String(from_amount.to_string());
        payload.metadata["basis_from_commodity"] =
            serde_json::Value::String(from_commodity.clone());

        eprintln!(
            "Basis: {} {} (via {}).",
            basis_amount, to_commodity, provider_display
        );
    }

    // Preview (best-effort) when we have enough information.
    if let (Some(provider), Some(rate), Some(base), Some(quote)) = (
        provider_display.clone(),
        payload.rate_context.override_rate,
        payload.rate_context.base.clone(),
        payload.rate_context.quote.clone(),
    ) {
        if let Some(quote_amount) = quote_amount_from_postings(&payload.postings, &quote) {
            if !rate.is_zero() {
                let value = (quote_amount / rate).round_dp(2);
                eprintln!(
                    "{} rate is {}. Transaction value: {} {}.",
                    provider, rate, value, base
                );
            }
        }
    }

    if !prompt_yes_no("Proceed? [Y/n] ")? {
        return Ok(());
    }

    db.insert_event(event_id, &payload)?;
    Ok(())
}

fn quote_amount_from_postings(postings: &[Posting], quote_commodity: &str) -> Option<Decimal> {
    // Prefer the outgoing amount in quote commodity (negative postings).
    let mut out = Decimal::ZERO;
    for p in postings {
        if p.commodity != quote_commodity {
            continue;
        }
        if p.amount.is_sign_negative() {
            out += -p.amount;
        }
    }
    if out > Decimal::ZERO {
        return Some(out);
    }

    // Fall back to incoming amount in quote commodity (positive postings).
    let mut incoming = Decimal::ZERO;
    for p in postings {
        if p.commodity != quote_commodity {
            continue;
        }
        if p.amount.is_sign_positive() {
            incoming += p.amount;
        }
    }
    if incoming > Decimal::ZERO {
        Some(incoming)
    } else {
        None
    }
}

fn primary_outgoing_amount(postings: &[Posting]) -> Option<(Decimal, String)> {
    // Sum outgoing amounts (negative postings) by commodity and return the largest.
    let mut by_commodity: BTreeMap<String, Decimal> = BTreeMap::new();
    for p in postings {
        if p.amount.is_sign_negative() {
            *by_commodity
                .entry(p.commodity.clone())
                .or_insert(Decimal::ZERO) += -p.amount;
        }
    }
    by_commodity
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1))
        .map(|(c, a)| (a, c))
}

/// Convert `amount` in `from` commodity into `to` commodity using the offline rate store.
///
/// Rates are stored as: (quote per base). This supports either:
/// - direct rate: base=from, quote=to => amount_to = amount_from * rate
/// - inverted rate: base=to, quote=from => amount_to = amount_from / rate
fn resolve_and_convert(
    db: &Db,
    provider: &str,
    from: &str,
    to: &str,
    as_of: DateTime<Utc>,
    amount: Decimal,
) -> Result<(Decimal, Decimal, bool, DateTime<Utc>)> {
    if from == to {
        return Ok((amount, Decimal::ONE, false, as_of));
    }

    if let Some((found_as_of, rate)) = db.get_rate_as_of(provider, from, to, as_of)? {
        return Ok((amount * rate, rate, false, found_as_of));
    }

    if let Some((found_as_of, rate)) = db.get_rate_as_of(provider, to, from, as_of)? {
        if rate.is_zero() {
            return Err(anyhow!("Stored rate is zero"));
        }
        return Ok((amount / rate, rate, true, found_as_of));
    }

    Err(anyhow!(
        "No stored rate for @{} between {} and {} at or before {}",
        provider,
        from,
        to,
        as_of.to_rfc3339()
    ))
}

fn prompt_yes_no(prompt: &str) -> Result<bool> {
    eprint!("{prompt}");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let s = line.trim();
    if s.is_empty() {
        return Ok(true);
    }
    Ok(matches!(s.to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn prompt_decimal(prompt: &str) -> Result<Option<Decimal>> {
    eprint!("{prompt}");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let s = line.trim();
    if s.is_empty() {
        return Ok(None);
    }
    Ok(Some(s.parse::<Decimal>().context("Invalid decimal")?))
}

fn print_balance(
    db: &Db,
    events: &[StoredEvent],
    account_prefix: Option<&str>,
    month_context: Option<&str>,
) -> Result<()> {
    let mut balances: BTreeMap<(String, String), Decimal> = BTreeMap::new();
    for e in events {
        for p in &e.payload.postings {
            if let Some(prefix) = account_prefix {
                if !p.account.starts_with(prefix) {
                    continue;
                }
            }
            let key = (p.account.clone(), p.commodity.clone());
            *balances.entry(key).or_insert(Decimal::ZERO) += p.amount;
        }
    }

    if balances.is_empty() {
        println!("(no balances)");
        return Ok(());
    }

    for ((acct, comm), amt) in &balances {
        println!("{acct}\t{comm}\t{amt}");
    }

    // Budget reservations (virtual deficits): only applies to budgets scoped to an account.
    // Month context: budget.month if present, else --month if provided, else current month.
    let budgets = db.list_budgets()?;
    if let Some(m) = month_context {
        let _ = parse_month_range(m)?;
    }
    let now_month = current_month_yyyy_mm(now_utc());
    let default_month = month_context.unwrap_or(&now_month);
    let mut reserved_budgets: BTreeMap<(String, String), Decimal> = BTreeMap::new();
    for b in budgets {
        let Some(acct) = &b.account else {
            continue;
        };
        if let Some(prefix) = account_prefix {
            if !acct.starts_with(prefix) {
                continue;
            }
        }

        let month = b.month.clone().unwrap_or_else(|| default_month.to_string());
        let (start, end) = parse_month_range(&month)?;
        let actual = compute_budget_actual(events, start, end, &b);
        let remaining_budget = b.amount - actual;
        if remaining_budget <= Decimal::ZERO {
            continue;
        }

        let reserve_amount = if let Some(from_prefix) = &b.auto_reserve_from {
            let until = b.auto_reserve_until_amount.unwrap_or(b.amount);
            let funded = compute_budget_funded(events, start, end, acct, &b.commodity, from_prefix)
                .min(until);
            let unspent_funded = (funded - actual).max(Decimal::ZERO);
            remaining_budget.min(unspent_funded)
        } else {
            remaining_budget
        };

        if reserve_amount <= Decimal::ZERO {
            continue;
        }
        let key = (acct.clone(), b.commodity.clone());
        *reserved_budgets.entry(key).or_insert(Decimal::ZERO) -= reserve_amount;
    }

    // Piggy reservations (virtual allocations): applies to the piggy's configured from_account.
    let piggies = db.list_piggies()?;
    let mut reserved_piggies: BTreeMap<(String, String), Decimal> = BTreeMap::new();
    for p in piggies {
        if let Some(prefix) = account_prefix {
            if !p.from_account.starts_with(prefix) {
                continue;
            }
        }

        let funded = db.piggy_funded_total(p.id)?;
        let reserved_amount = funded.min(p.target_amount);
        if reserved_amount <= Decimal::ZERO {
            continue;
        }

        let key = (p.from_account.clone(), p.commodity.clone());
        *reserved_piggies.entry(key).or_insert(Decimal::ZERO) -= reserved_amount;
    }

    let has_any_reserved = !(reserved_budgets.is_empty() && reserved_piggies.is_empty());

    if has_any_reserved {
        if !reserved_budgets.is_empty() {
            println!();
            println!("(reserved budgets)");
            for ((acct, comm), amt) in &reserved_budgets {
                println!("{acct}\t{comm}\t{amt}");
            }
        }

        if !reserved_piggies.is_empty() {
            println!();
            println!("(reserved piggies)");
            for ((acct, comm), amt) in &reserved_piggies {
                println!("{acct}\t{comm}\t{amt}");
            }
        }

        println!();
        println!("(effective balance)");
        let mut effective: BTreeMap<(String, String), Decimal> = BTreeMap::new();
        for (k, v) in &balances {
            effective.insert(k.clone(), *v);
        }
        for (k, v) in &reserved_budgets {
            *effective.entry(k.clone()).or_insert(Decimal::ZERO) += *v;
        }
        for (k, v) in &reserved_piggies {
            *effective.entry(k.clone()).or_insert(Decimal::ZERO) += *v;
        }

        for ((acct, comm), amt) in &effective {
            println!("{acct}\t{comm}\t{amt}");
        }
    }
    Ok(())
}

fn filter_events(
    events: &[StoredEvent],
    args: &crate::cli::ReportArgs,
) -> Result<Vec<StoredEvent>> {
    let mut out = Vec::new();

    let month_range = if let Some(m) = &args.month {
        Some(parse_month_range(m)?)
    } else {
        None
    };

    let explicit_range = if let Some(r) = &args.range {
        Some(parse_date_range(r)?)
    } else {
        None
    };

    for e in events {
        if let Some((start, end)) = month_range {
            if e.effective_at < start || e.effective_at > end {
                continue;
            }
        }
        if let Some((start, end)) = explicit_range {
            if e.effective_at < start || e.effective_at > end {
                continue;
            }
        }
        if let Some(acct) = &args.account {
            let any = e
                .payload
                .postings
                .iter()
                .any(|p| p.account.starts_with(acct));
            if !any {
                continue;
            }
        }
        if let Some(cat) = &args.category {
            if e.payload.category.as_deref() != Some(cat.as_str()) {
                continue;
            }
        }
        if let Some(tag) = &args.tag {
            if !e.payload.tags.iter().any(|t| t == tag) {
                continue;
            }
        }
        if let Some(comm) = &args.commodity {
            let any = e.payload.postings.iter().any(|p| p.commodity == *comm);
            if !any {
                continue;
            }
        }

        out.push(e.clone());
    }
    Ok(out)
}

fn print_report(events: &[StoredEvent]) {
    if events.is_empty() {
        println!("(no events)");
        return;
    }
    for e in events {
        println!(
            "{}\t{}\t{}",
            e.effective_at.to_rfc3339(),
            e.action,
            e.event_id
        );
    }
}

fn parse_month_range(raw: &str) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let (y, m) = raw
        .split_once('-')
        .ok_or_else(|| anyhow!("Invalid --month. Expected YYYY-MM"))?;
    let year: i32 = y.parse()?;
    let month: u32 = m.parse()?;
    if !(1..=12).contains(&month) {
        return Err(anyhow!("Invalid month value"));
    }
    let start_date =
        NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| anyhow!("Invalid date"))?;
    let start = Utc.from_utc_datetime(&NaiveDateTime::new(
        start_date,
        NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    ));
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let next_start_date =
        NaiveDate::from_ymd_opt(next_year, next_month, 1).ok_or_else(|| anyhow!("Invalid date"))?;
    let end = Utc.from_utc_datetime(&NaiveDateTime::new(
        next_start_date,
        NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    )) - chrono::Duration::seconds(1);
    Ok((start, end))
}

fn parse_date_range(raw: &str) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let (start, end) = raw
        .split_once("..")
        .ok_or_else(|| anyhow!("Invalid --range. Expected YYYY-MM-DD..YYYY-MM-DD"))?;
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d")?;
    let start_dt = Utc.from_utc_datetime(&NaiveDateTime::new(
        start,
        NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    ));
    let end_dt = Utc.from_utc_datetime(&NaiveDateTime::new(
        end,
        NaiveTime::from_hms_opt(23, 59, 59).unwrap(),
    ));
    Ok((start_dt, end_dt))
}
