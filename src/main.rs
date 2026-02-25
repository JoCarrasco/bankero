mod cli;
mod config;
mod db;
mod domain;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use clap::Parser;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::io::{self, Write};
use uuid::Uuid;

use crate::cli::{parse_provider_opt, Cli, Command, ProjectCmd, WsCmd};
use crate::config::{app_paths, load_or_init_config, now_utc, write_config, AppConfig};
use crate::db::Db;
use crate::domain::{
    parse_basis_arg, BasisContext, EventPayload, Posting, ProviderToken, RateContext, StoredEvent,
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
        Command::Ws(args) => {
            handle_ws(args.cmd, &paths, &mut cfg, &cfg_path)?;
            return Ok(());
        }
        Command::Project(args) => {
            handle_project(args.cmd, &paths, &mut cfg, &cfg_path)?;
            return Ok(());
        }
        _ => {}
    }

    let (db, db_path) = Db::open(&paths, &cfg.current_workspace)?;

    match cli.command {
        Command::Deposit(args) => {
            let confirm = args.common.confirm;
            let event_id = Uuid::new_v4();
            let payload = build_deposit_event(&cfg, "deposit", event_id, args.amount, args.commodity, args.from, args.to, None, args.common)?;
            maybe_confirm_and_insert(&db, event_id, &payload, confirm)?;
            println!("Wrote event {event_id} to {}", db_path.display());
        }
        Command::Move(args) => {
            let (to_amount, to_commodity, provider) = parse_move_tail(&args.tail)?;
            let confirm = args.common.confirm;
            let event_id = Uuid::new_v4();
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
            maybe_confirm_and_insert(&db, event_id, &payload, confirm)?;
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
            maybe_confirm_and_insert(&db, event_id, &payload, confirm)?;
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
            maybe_confirm_and_insert(&db, event_id, &payload, confirm)?;
            println!("Wrote event {event_id} to {}", db_path.display());
        }
        Command::Tag(args) => {
            let confirm = args.common.confirm;
            let event_id = Uuid::new_v4();
            let payload = build_tag_event(&cfg, event_id, args.target, args.set_basis, args.common)?;
            maybe_confirm_and_insert(&db, event_id, &payload, confirm)?;
            println!("Wrote event {event_id} to {}", db_path.display());
        }
        Command::Balance(args) => {
            let events = db.list_events()?;
            print_balance(&events, args.account.as_deref())?;
        }
        Command::Report(args) => {
            let events = db.list_events()?;
            let filtered = filter_events(&events, &args)?;
            print_report(&filtered);
        }
        Command::Budget(_cmd) => {
            eprintln!("budget commands are not implemented yet (Milestone 6)");
        }
        Command::Task(_) | Command::Workflow(_) | Command::Login | Command::Sync(_) | Command::Piggy(_) => {
            eprintln!("This command is a stub for later milestones.");
        }
        Command::Ws(_) | Command::Project(_) => {
            unreachable!();
        }
    }

    Ok(())
}

fn handle_ws(cmd: WsCmd, paths: &crate::config::AppPaths, cfg: &mut AppConfig, cfg_path: &std::path::Path) -> Result<()> {
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

fn handle_project(cmd: ProjectCmd, paths: &crate::config::AppPaths, cfg: &mut AppConfig, cfg_path: &std::path::Path) -> Result<()> {
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

fn parse_move_tail(tail: &[String]) -> Result<(Option<Decimal>, Option<String>, Option<ProviderToken>)> {
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
            let to_amount = parse_decimal(tail[0].clone(), "to_amount")?;
            let to_commodity = tail[1].clone();
            Ok((Some(to_amount), Some(to_commodity), None))
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

fn parse_as_of(common: &crate::cli::CommonEventFlags, effective_at: DateTime<Utc>) -> Result<DateTime<Utc>> {
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
            let inferred_rate = if amount.is_zero() { None } else { Some(to_amount / amount) };
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

    let inferred_rate = if amount.is_zero() { None } else { Some(to_amount / amount) };

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

fn maybe_confirm_and_insert(db: &Db, event_id: Uuid, payload: &EventPayload, confirm: bool) -> Result<()> {
    if !confirm {
        db.insert_event(event_id, payload)?;
        return Ok(());
    }

    let mut payload = payload.clone();
    let provider = payload.rate_context.provider.clone();

    if provider.is_some() && payload.rate_context.override_rate.is_none() && payload.rate_context.base.is_some() && payload.rate_context.quote.is_some() {
        let rate = prompt_decimal(&format!(
            "Enter rate for {} ({} per {}) or blank to skip: ",
            provider.clone().unwrap_or_else(|| "@provider".to_string()),
            payload.rate_context.quote.as_deref().unwrap_or("quote"),
            payload.rate_context.base.as_deref().unwrap_or("base"),
        ))?;
        if let Some(rate) = rate {
            payload.rate_context.override_rate = Some(rate);
        }
    }

    // Preview (best-effort) when we have enough information.
    if let (Some(provider), Some(rate), Some(base), Some(quote)) = (
        provider.clone(),
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

fn print_balance(events: &[StoredEvent], account_prefix: Option<&str>) -> Result<()> {
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

    for ((acct, comm), amt) in balances {
        println!("{acct}\t{comm}\t{amt}");
    }
    Ok(())
}

fn filter_events(events: &[StoredEvent], args: &crate::cli::ReportArgs) -> Result<Vec<StoredEvent>> {
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
        println!("{}\t{}\t{}", e.effective_at.to_rfc3339(), e.action, e.event_id);
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
    let start_date = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| anyhow!("Invalid date"))?;
    let start = Utc.from_utc_datetime(&NaiveDateTime::new(start_date, NaiveTime::from_hms_opt(0, 0, 0).unwrap()));
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let next_start_date = NaiveDate::from_ymd_opt(next_year, next_month, 1).ok_or_else(|| anyhow!("Invalid date"))?;
    let end = Utc.from_utc_datetime(&NaiveDateTime::new(next_start_date, NaiveTime::from_hms_opt(0, 0, 0).unwrap()))
        - chrono::Duration::seconds(1);
    Ok((start, end))
}

fn parse_date_range(raw: &str) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let (start, end) = raw
        .split_once("..")
        .ok_or_else(|| anyhow!("Invalid --range. Expected YYYY-MM-DD..YYYY-MM-DD"))?;
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d")?;
    let start_dt = Utc.from_utc_datetime(&NaiveDateTime::new(start, NaiveTime::from_hms_opt(0, 0, 0).unwrap()));
    let end_dt = Utc.from_utc_datetime(&NaiveDateTime::new(end, NaiveTime::from_hms_opt(23, 59, 59).unwrap()));
    Ok((start_dt, end_dt))
}
