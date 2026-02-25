# Bankero

Local-first ledger for multi-currency money movement with time-based conversion, intrinsic value tracking, monthly budgets, and powerful reporting.

Bankero is CLI-first: your ledger lives on your machine, stays usable offline, and remains auditable across devices.

## Goals (product principles)

- **Local-first by default**: data is stored locally; network features (if any) are optional.
- **Auditable**: changes should be explainable and traceable (ideally append-only journal or well-defined history).
- **Multi-currency, time-aware**: conversions use an *as-of timestamp* with explicit rate selection semantics.
- **Predictable math**: clear rounding rules, currency precision, and reproducible totals.

## Install

### Fedora/RHEL (RPM)

Bankero publishes an `.rpm` artifact on each GitHub Release.

#### 1) Download the RPM from GitHub Releases

- Go to: `https://github.com/JoCarrasco/bankero/releases`
- Download the latest `bankero-<version>-1.x86_64.rpm`

#### 2) Install

```bash
sudo dnf install ./bankero-*.rpm
bankero --help
```

#### Upgrade

Download the newer RPM and run:

```bash
sudo dnf upgrade ./bankero-*.rpm
```

Note: this is a direct RPM install (no `dnf` repo yet), so upgrades require downloading the new RPM.

### Debian/Ubuntu (APT)

This project can be distributed via a signed APT repository hosted on GitHub Pages.

Repo URL (GitHub Pages):

- `https://jocarrasco.github.io/bankero/apt`

#### 1) Add the signing key

Download and install the repository public key into a dedicated keyring:

```bash
curl -fsSL https://jocarrasco.github.io/bankero/apt/public.gpg \
	| sudo gpg --dearmor -o /usr/share/keyrings/bankero-archive-keyring.gpg
```

#### 2) Add the apt source

```bash
echo "deb [signed-by=/usr/share/keyrings/bankero-archive-keyring.gpg] https://jocarrasco.github.io/bankero/apt stable main" \
	| sudo tee /etc/apt/sources.list.d/bankero.list
sudo apt-get update
```

#### 3) Install

```bash
sudo apt-get install bankero
bankero --help
```

#### Upgrade

Once installed via APT, you can upgrade with the built-in helper:

```bash
bankero upgrade
bankero upgrade --apply
```

If you haven't configured the repo yet, this will set it up and then upgrade:

```bash
bankero upgrade --setup-apt --apply
```

#### Publishing notes

- The GitHub Actions release workflow expects repository secrets:
	- `APT_GPG_PRIVATE_KEY` (ASCII-armored private key)
	- `APT_GPG_PASSPHRASE` (optional; only needed if the private key is passphrase-protected)
- The workflow exports the public key automatically to `https://jocarrasco.github.io/bankero/apt/public.gpg`.

## Non-goals (for now)

- Building a hosted SaaS.
- Replacing full accounting suites on day one.

## What Bankero does

Bankero helps you track money movement over time in a way that remains useful offline:

- Record transactions (who/what/when/how much)
- Track balances across accounts
- Work in multiple currencies
- Convert amounts using an **as-of timestamp** (so historical reports remain consistent)
- Set **monthly budgets** and compare budget vs actual
- Tag/categorize activity so reports can be filtered
- Run **recurrent tasks** (cron-like) that trigger webhooks and append workflow events to the ledger

## Roadmap (implementation status)

As of 2026-02-25:

- [x] Rust CLI scaffold (`cargo run -- <command> ...`)
- [x] Local-first persistence with per-workspace SQLite DB (append-only `events` journal)
- [x] Core actions write immutable events: `deposit`, `move`, `buy`, `sell`, `tag`
- [x] Basic read models by replay: `balance` (actual only) and `report` (filters: month/range/account/category/tag/commodity)
- [x] Workspace switching (`ws add|checkout|check`) with complete data isolation per workspace
- [x] Project checkout stored in config and recorded on events (project list/spend rollups pending)
- [x] Integration tests to freeze current CLI behavior (runs against a temporary `BANKERO_HOME`, safe for parallel runs; includes cross-flow scenarios like ws/project/budget)
- [x] E2E “flows matrix” tests (cross-command workflows: ws isolation, rates roundtrip, sell confirm, tag+report)
- [x] GitHub Actions guardrails: CI (fmt+tests), Clippy (correctness/suspicious), Coverage summary, Nightly run
- [x] Offline provider rate store (`bankero rate set|get|list`)
- [x] `--confirm` uses stored provider rates and prints a value preview (`move ... @provider --confirm`)
- [x] Computed cross-currency `move` using stored rates (`move 100 USD ... VES @provider`)
- [x] `--confirm` computes basis deterministically when using `-b @provider` (requires stored rates)

Next up (in PRD order):

- [ ] Full provider-backed conversion engine (compute missing legs, deterministic provider logic)
- [ ] `--confirm` flows that preview computed conversions/basis before writing
- [x] Monthly budgets (create + budget-vs-actual report)
- [x] Effective balance (reserved vs effective) for account-scoped budgets
- [x] Budget automation MVP: auto-reserve from matching credits (cap with `--until`)
- [ ] Piggy banks (savings goals)
- [ ] Multi-device sync (`login`, `sync status|now`)
- [ ] Recurrent tasks + workflows + webhook integrations

## Flow checklist (E2E use-cases)

This project prioritizes **flow correctness** over pure line coverage: we track whether real CLI workflows (cross-over use cases) keep working end-to-end.

Scope: only flows that are implemented (stub commands like `sync`, `task`, `workflow`, `piggy` are excluded until they stop being stubs).

Compute flow coverage from the terminal:

```bash
bash scripts/flows_coverage.sh
# or enforce a minimum threshold:
bash scripts/flows_coverage.sh --min 80
```

Run the full E2E flow test suite:

```bash
cargo e2e
```

- [x] Workspace isolation (events + rates) — `tests/flows_e2e.rs::workspace_isolation_applies_to_events_and_rates`
- [x] Workspace/project switching + reset semantics — `tests/flows_e2e.rs::ws_check_and_project_checkout_work_and_ws_checkout_resets_project`
- [x] Provider rate store roundtrip (`rate set|get|list`) — `tests/flows_e2e.rs::rate_set_get_list_roundtrip_is_deterministic`
- [x] Deposit → balance rebuild — `tests/cli_smoke.rs::deposit_and_move_write_events_and_balance_rebuilds`
- [x] Move (manual override cross-currency) → balance — `tests/cli_smoke.rs::deposit_and_move_write_events_and_balance_rebuilds`
- [x] Move (computed quote from stored rate) → balance — `tests/cli_smoke.rs::move_can_compute_quote_amount_from_stored_rate`
- [x] Confirm-mode preview + commit (move) — `tests/confirm_flow.rs::confirm_mode_uses_stored_rate_and_prints_value_preview`
- [x] Confirm-mode basis computation (`-b @provider`) — `tests/confirm_flow.rs::confirm_mode_computes_basis_deterministically_when_basis_provider_is_set`
- [x] Buy with splits (valid) + split validation failure — `tests/cli_smoke.rs::buy_with_splits_requires_sum_match`
- [x] Sell confirm-mode preview + commit — `tests/flows_e2e.rs::sell_confirm_flow_writes_event_and_prints_value_preview`
- [x] Tag with fixed basis + report tag filter — `tests/flows_e2e.rs::tag_fixed_basis_is_recorded_and_report_can_filter_by_tag`
- [x] Report filters: month/category/tag — `tests/cli_smoke.rs::report_filters_by_month_category_and_tag`
- [x] Report filters: range/account/commodity — `tests/flows_e2e.rs::report_filters_by_range_account_and_commodity`
- [x] Budgets: create + report actuals — `tests/budget_flow.rs::budget_create_and_report_shows_actual_spend_for_month`
- [x] Budgets: effective balance (reserved + effective) — `tests/budget_flow.rs::balance_shows_reserved_and_effective_for_account_scoped_budgets`
- [x] Budgets: automation (funded cap minus spend) — `tests/budget_flow.rs::auto_reserve_reserves_only_funded_amount_minus_spend`

## Concepts

- **Workspace**: the highest-level context (e.g., Personal, Business). Switching workspaces swaps the entire local data environment.
- **Project**: a sub-context inside a workspace (e.g., Startup, House Remodel). When a project is active, actions are automatically tagged to it.
- **Accounts**: hierarchical names like `assets:banesco`, `income:freelance`, `expenses:food:groceries`.
- **Commodities**: currencies and assets like `USD`, `VES`, `EUR`, `USDT`, `XAU`.
- **Actions**: verb-first commands that encode intent (`deposit`, `move`, `buy`, `sell`, `tag`).
- **Providers**: `@provider` tokens that identify exchange-rate sources.
- **Basis (intrinsic value)**: optional metadata capturing “real value” at a market rate, separate from the nominal amount.
- **Virtuals (piggy banks & budgets)**: overlays that change your **effective balance** without creating real ledger movements.
- **Budgets**: monthly targets that reserve money as a **virtual deficit** (budget vs actual, plus effective balance).
- **Tags & categories**: metadata used to slice reports without changing the accounting logic.
- **Recurrent tasks**: named scheduled jobs (with an id) that trigger a webhook.
- **Workflows**: event-driven pipelines that transform external inputs into append-only ledger events.

## Hierarchy & context

Bankero isolates different financial lives using a tiered structure:

- **Workspace**: top-level container (e.g., Personal, Business). Switching workspaces changes the entire database.
- **Project**: sub-context in a workspace (e.g., Startup, House Remodel). Projects help attribute spending while keeping shared accounts.
- **Accounts**: your real buckets (e.g., `assets:bank`).
- **Virtuals**: piggy banks and budgets that alter your *effective* balance without moving money.

## Workspace & project management

Every installation includes a default workspace named `personal`.

### Workspace commands

```bash
# Check current context
bankero ws check
> "You are currently in workspace: Personal"
> "and the current project is: 'default'"

# Add and switch workspaces
bankero ws add "Startup-X"
bankero ws checkout "Startup-X"
```

### Project commands

Projects group transactions into initiatives. When a project is active, every `buy`/`move` is automatically tagged to it.

```bash
# Add a specific goal or initiative
bankero project add "Fix roof"

# Switch to the project (auto-tags all future transactions)
bankero project checkout "Fix roof"

# List projects and their current spending
bankero project list
```

## Piggy banks (savings goals)

A piggy bank is a specialized virtual that tracks progress toward a target amount and can auto-fund from other accounts.

```bash
# Create a Piggy Bank for a new car
bankero piggy create "New Car" 5000 USD --from assets:savings

# Check progress (shows percentage and remaining)
bankero piggy status "New Car"
> [====------] 40% ($2,000 / $5,000)
```

## Getting started

Bankero is used by writing transactions as explicit actions:

```bash
bankero <action> <amount> <commodity> --from <account> --to <account> [flags]
```

## CLI

### UX design philosophy

The structure is based on three core principles:

- **Verb-first logic**: instead of a generic `txn create`, use specific financial actions (`buy`, `move`, `deposit`) to reduce typing and improve readability.
- **The `@` convention**: `@provider` denotes price/rate providers.
- **The basis (intrinsic) flag**: in volatile economies, the nominal value (what you paid) often differs from the intrinsic value (e.g., USD value at a market rate). Use `--basis` / `-b` to track this metadata.

### Command structure

```bash
bankero <action> <amount> <commodity> --from <account> --to <account> [flags]
```

### Key flags

- `@provider`: specifies the exchange rate source (e.g., `@bcv`, `@binance`, `@parallel`).
- `@provider:rate`: overrides the provider’s current price with a specific manual value.
- `--basis` / `-b`: sets the intrinsic value (fixed amount or a provider to auto-calculate).
- `--tag <name>`: repeatable free-form tags for filtering reports (e.g., `--tag groceries --tag family`).
- `--category <path>`: a primary category for budgets and rollups (e.g., `expenses:food:groceries`).
- `--note`, `-m`: free-form note/memo.
- `--confirm`: resolves required provider rates from the local rate store and asks for confirmation before writing.

### Provider rates (offline)

If you use `@provider` without an explicit override like `@provider:rate`, you can store provider rates locally:

```bash
bankero rate set @bcv USD VES 45.2 --as-of 2026-02-25T12:00:00Z
bankero rate get @bcv USD VES --as-of 2026-02-25T12:00:00Z
bankero rate list @bcv          # latest rate for each known pair
bankero rate list @bcv USD      # latest rate for each quote for this base
bankero rate list @bcv USD VES  # history for a specific pair

# For script-friendly output:
bankero rate list @bcv USD VES --format tsv
```

### Usage examples

1) Simple income recording

```bash
bankero deposit 1500 USD --to assets:savings --from income:freelance -m "Web project payout"
```

2) Multi-currency transfer (manual rate)

```bash
bankero move 100 USD --from assets:wells-fargo --to assets:banesco 42000 VES @manual:420
```

2a) Multi-currency transfer (stored provider rate)

```bash
bankero rate set @bcv USD VES 45.2 --as-of 2026-02-25T12:00:00Z
bankero move 100 USD --from assets:wells-fargo --to assets:banesco VES @bcv
```

3) Purchasing with auto-rate (BCV)

```bash
bankero buy external:traki 2500 VES --from assets:banesco @bcv --note "New clothes"
```

4) Tracking intrinsic value (market rate)

```bash
bankero buy external:farmatodo 840 VES --from assets:mercantil @bcv -b @binance
```

5) Crypto-to-fiat exit

```bash
bankero sell 100 USDT --to assets:banesco 4500 VES @binance --note "P2P Exit"
```

6) Expense with custom intrinsic value

```bash
bankero buy external:landlord 12000 VES --from assets:cash --basis 300 USD
```

7) Splitting a bill

```bash
bankero buy 500 USD --from assets:bank --to expenses:rent:450 --to expenses:water:50
```

8) Intrinsic update (revaluation)

```bash
bankero tag assets:gold-bar --set-basis "2000 USD" --note "Monthly revaluation"
```

9) Interactive confirm mode

```bash
bankero move 5000 VES --from assets:wallet --to external:neighbor @binance --confirm
# Result: > Binance rate is 45.2. Transaction value: 110.61 USD. Proceed? [Y/n]
```

10) Recording a liability

```bash
bankero buy assets:new-laptop 1200 USD --from liabilities:credit-card -b @binance
```

### Why this structure?

**Position vs. flag**

`--from` and `--to` are flags rather than positional args. For financial data, clarity beats speed: it reduces the chance of accidentally reversing the direction.

**Tokenizing the rate (`@`)**

`@bcv` is a provider, `@bcv:36.5` is provider + manual override. This keeps the CLI dense and makes parsing unambiguous.

**Intrinsic value as a first-class citizen**

Most ledger tools treat cost basis as an afterthought. In Bankero, `--basis` helps track “real value” (often USD/Gold) while operating day-to-day in a volatile currency.

## Time-based conversion

Conversions are explicit about *when* they apply.

- Rates have an **effective timestamp** and an optional source.
- Conversions resolve rates deterministically for a given as-of time.

Example:

- Record: `USD->EUR = 0.92` effective `2026-02-25T10:00:00Z`
- Convert: `100 USD` to `EUR` as-of `2026-02-25T12:00:00Z` → `92 EUR`

## Data model (early draft)

These are working definitions to help us design the first version:

- **Ledger**: collection of accounts, transactions, and rates.
- **Account**: named bucket that holds positions (optionally scoped by currency/commodity).
- **Transaction**: time-stamped event with one or more postings/legs.
- **Posting/Leg**: movement of an amount in a currency from/to an account.
- **Rate**: exchange rate between two currencies effective at a given timestamp.
- **Category/Tag**: metadata attached to events for reporting (budgets, filters, rollups).
- **Monthly budget**: a month-scoped plan, typically per category (and optionally per account), in a target commodity.

Open questions we’ll refine:

- Double-entry vs “simple ledger” mode?
- How to represent fees, spreads, and rounding?
- How to store history: append-only journal vs snapshots?

## Storage (local-first)

Storage traits:

- Human-friendly and backup-friendly
- Cross-platform
- Supports migrations as the schema evolves

Bankero persists data locally using:

- **SQLite** for durable storage
- **An immutable event journal** (append-only) plus rebuildable **projections**

## Multi-device sync

Bankero supports many devices while preserving local-first behavior and a complete audit trail.

### Why “last write wins” breaks ledgers

Many sync systems resolve conflicts with **last write wins (LWW)**. For a financial ledger this is a non-starter: if two devices record offline transactions concurrently, LWW can overwrite one and destroy the audit trail.

### Data strategy: event sourcing + append-only journal

Bankero uses **event sourcing**:

- Instead of storing “current balance of `assets:banesco`”, store the **events** that led there (e.g., `FundsDeposited`, `CurrencyMoved`, `CommodityBought`, `BasisUpdated`).
- Local storage keeps an **immutable events table** (the journal) plus **projections** (materialized views like balances, positions, last-known basis).
- Conflict resolution becomes predictable because you don’t merge *state*; you merge **lists of immutable events**.

At a high level:

- Each device appends new events locally.
- Sync exchanges missing events and rebuilds projections.
- Because events are append-only, merging remains deterministic and auditable.

### Stack (local sync)

Bankero synchronizes devices using:

- **Loro** as the conflict-free replicated state layer
- **Iroh** as the transport layer, using mDNS for Wi‑Fi discovery and peer-to-peer connections

This combination provides convergence without last-write-wins and keeps the ledger auditable.

### CLI

```bash
bankero login
bankero sync status
bankero sync now
```

### Core architecture: ports & adapters + domain invariants

To support a CLI, a sync server, an API, and webhooks without duplicating business logic, structure the app with **hexagonal architecture (ports & adapters)**:

- **Driving adapters (primary)**: CLI commands (`bankero buy`, `bankero move`, `bankero deposit`), API endpoints, webhook processors.
- **Driven adapters (secondary)**: local storage, rate provider clients, and the network sync client.

Keep a small, isolated domain core where **domain invariants** live (double-entry rules, multi-currency semantics, rounding, explicit rate selection, and `--basis` rules). Regardless of where a transaction originates, it must pass through the same validations to guarantee predictable math.

### Sync server role, auth, and webhooks

The server doesn’t perform heavy financial calculations. Its primary jobs are to:

- Authenticate devices and authorize access to a ledger
- Receive batches of events and serve missing events
- Act as a durable archive and a routing hub

For onboarding many devices:

- `bankero login` uses an OAuth 2.0 device authorization flow.
- Access/refresh tokens are stored locally.

Because the server sees an ordered stream of events, it powers **webhooks** (e.g., notify when USDT activity occurs, or when intrinsic value crosses a threshold).

### Cloud peer (backup + relay)

Bankero treats the **local network as the primary sync layer** and the **cloud as a delayed, batched backup**.

- The cloud node runs a **Rust + Axum** service with an embedded **Iroh** peer (always-on relay).
- The cloud stores compacted, finalized replicated-state blobs in **Postgres**.
- Devices push a single compacted Loro binary blob when the cloud peer is reachable.

Iroh requires **UDP** for fast P2P hole punching; Bankero deploys the cloud node on **Fly.io**, which supports raw UDP routing.

Local development uses **Docker Compose** to run Postgres and the Axum backend in reproducible containers.

## Budgets

Bankero supports monthly budgets by category with budget-vs-actual reporting.

Bankero also supports an **effective balance** overlay for budgets that are scoped to an `--account`.

### Creating a budget

```bash
bankero budget create "Materials" 1500 USDT @binance --account assets:personalbinance
Created budget 'Materials' 1500 USDT.
```

Report budgets vs actual spend:

```bash
bankero budget report --month 2026-02
```

### Automation: virtual siphoning

Virtually reserve money when specific credits happen, so budgets are funded before you see the “available” cash.

```bash
# Every time I get paid from salary, virtually reserve money into the budget
# (capped by --until; reservation is reduced as you spend in the budget category)
bankero budget update "Food" --auto-reserve-from income:salary --until 200 USD
```

Disable automation:

```bash
bankero budget update "Food" --clear-auto-reserve
```

### Checking balance (actual vs effective)

```bash
bankero balance assets:personalbinance
> Actual Balance:    2500.00 USDT
> Budget Reserved:  -1300.00 USDT (Materials)
> Effective Total:   1200.00 USDT
```

Note: effective balance currently accounts for **account-scoped budgets** only (budgets created with `--account`).

You can set the month context used for budget reservations with `--month`:

```bash
bankero balance assets:bank --month 2026-02
```

Example:

```bash
bankero budget create "Food" 300 USD --month 2026-02 --category expenses:food
bankero budget report --month 2026-02
```

## Usage UX summary

| Concept | Action | Impact |
| --- | --- | --- |
| Workspace | `ws checkout` | Swaps the entire data environment |
| Project | `project checkout` | Auto-tags transactions to a specific goal |
| Piggy bank | `piggy fund` | Tracks progress of a target amount |
| Budget | `budget create` | Creates a budget target used by `budget report` |

## Reports

Reports are filterable by account, category, tag, commodity, and date range.

Examples:

```bash
bankero report --month 2026-02
bankero report --category expenses:food --month 2026-02
bankero report --tag groceries --month 2026-02
```

## Recurrent tasks & workflows

Bankero includes a cron-like scheduler for integrations. A **recurrent task** has a stable id + name, a schedule, and a target webhook. Each execution produces a workflow run with its own events, and any resulting financial changes are appended to the same immutable journal.

Example: pull Payoneer transactions every 30 minutes, transform them, and append ledger events:

```bash
bankero task create payoneer-sync --every 30m --webhook https://example.local/bankero/hooks/payoneer
bankero task enable payoneer-sync
```

Inspect and operate:

```bash
bankero task list
bankero task run payoneer-sync
bankero workflow runs --task payoneer-sync --last 10
bankero workflow events --run <run-id>
```

Workflows are composable: a workflow can append additional workflow events (Zapier/n8n-style) to enrich, classify, or split transactions before committing.

## Contributing

Open issues with ideas, edge cases, and desired workflows. PRs welcome.

## License

See `package.json` for the current license value.
