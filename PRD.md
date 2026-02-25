# Bankero — Product Requirements Document (PRD)

## 1. Summary

**Bankero** is a local-first, multi-currency ledger that stays fully usable offline, synchronizes across many devices without losing audit history, and produces predictable, reproducible financial math.

Bankero is action-driven (verb-first) and optimized for high-signal CLI workflows in volatile, multi-rate economies.

## 2. Problem statement

People who operate across multiple currencies (e.g., USD/VES/USDT/EUR) need:

- A ledger that works offline and remains trustworthy
- Explicit time-based conversion so historical reports don’t drift
- The ability to track **intrinsic value** (market value) separately from nominal value
- Robust syncing across many devices without “last write wins” data loss
- Monthly budgets and fast reporting filtered by tags/categories

## 3. Goals

- Record auditable financial events with deterministic results
- Support multi-currency actions with explicit rate providers and overrides
- Track intrinsic value (basis) as first-class metadata
- Provide budget-vs-actual reporting by month and category
- Provide fast, filterable reports (account, category, tag, commodity, date range)
- Support recurrent tasks that trigger webhooks and run composable workflows
- Sync across 20+ devices without conflict and without losing events

## 4. Non-goals

- A hosted SaaS-first experience
- Full enterprise accounting parity on day one
- “Magical” implicit conversions without an as-of timestamp / provider context

## 5. Target users

- Individuals in multi-currency economies
- Freelancers/contractors paid in foreign currencies
- Crypto users moving between fiat and stablecoins
- Small teams that need shared visibility without centralizing the primary ledger authoring

## 6. UX principles

1. **Verb-first logic**
   - Use financial actions (`deposit`, `move`, `buy`, `sell`, `tag`) instead of generic `txn create`.

2. **`@provider` convention**
   - `@provider` declares a rate source; `@provider:rate` is a source + explicit override.

3. **Intrinsic value as metadata**
   - `--basis/-b` captures intrinsic value (often USD) regardless of nominal currency.

4. **Safe defaults**
   - Use `--from/--to` flags to prevent accidental direction reversal.
   - Support `--confirm` for any action requiring fetched rates.

## 7. Core concepts & terminology

- **Account**: hierarchical label (e.g., `assets:banesco`, `income:freelance`, `expenses:food:groceries`).
- **Commodity**: a currency or asset symbol (e.g., `USD`, `VES`, `USDT`, `XAU`).
- **Workspace**: top-level context (e.g., Personal, Business). Switching workspaces swaps the entire data environment.
- **Project**: sub-context within a workspace (e.g., Startup, House Remodel). Active project auto-tags subsequent actions.
- **Virtuals**: overlays (piggy banks and budgets) that affect effective balance without creating real ledger movements.
- **Event**: an append-only record representing a user intent (deposit/move/buy/sell/tag/budget).
- **Projection**: a derived view (balances, positions, budget vs actual) computed from events.
- **Provider**: an identifier describing how a rate is determined (`@bcv`, `@binance`, `@parallel`, `@manual`).
- **Basis (intrinsic value)**: the “real value” of the action, usually in a reference commodity (commonly USD), computed or specified.
- **Recurrent task**: a named scheduled job (with a stable id) that triggers a webhook.
- **Workflow run**: a structured execution that emits workflow events and may result in ledger events.

## 8. Primary workflows (user stories)

### 8.1 Recording income
- As a user, I record receiving money into an account with a memo.
- As a user, I can report monthly income grouped by category/tags.

### 8.2 Moving value between accounts
- As a user, I move value from one account to another in the same commodity.
- As a user, I can move across currencies using a manual or provider rate.

### 8.3 Buying goods/services
- As a user, I record a purchase (expense) in local currency.
- As a user, I attach intrinsic value using a market provider.

### 8.4 Selling a commodity
- As a user, I sell USDT (or another commodity) into a fiat account using a provider.

### 8.5 Revaluation
- As a user, I update the basis of an asset without moving currency.

### 8.6 Budgets
- As a user, I set budgets per category per month.
- As a user, I view budget vs actual for a month.
- As a user, my account summary shows an effective balance that reserves budgeted money without moving funds.

### 8.6.1 Piggy banks (savings goals)
- As a user, I create a piggy bank with a target amount.
- As a user, I can see progress (percent, remaining) and optionally auto-fund from an account.

### 8.6.2 Workspaces & projects
- As a user, I separate finances into workspaces (Personal/Business) and switch between them.
- As a user, I create projects and auto-tag actions to an active project.

### 8.7 Reporting
- As a user, I filter reports by month, account prefix, category, tag, commodity, and date range.

### 8.8 Multi-device sync
- As a user, I create events offline on multiple devices.
- As a user, syncing never deletes events and preserves audit trail.

### 8.9 Recurrent tasks & integrations
- As a user, I create a recurrent task (e.g., “Fetch Payoneer transactions every 30 minutes”).
- As a user, each execution has a workflow run id and an auditable stream of workflow events.
- As a user, workflows transform external data and append deterministic ledger events.

### 8.10 Usage examples by workflow

These examples map directly to the primary workflows above.

**8.1 Recording income**

```bash
bankero deposit 1500 USD --to assets:savings --from income:freelance -m "Web project payout" --category income:freelance --tag client:acme
```

**8.2 Moving value between accounts**

Same-currency move:

```bash
bankero move 200 USD --from assets:wallet --to assets:bank --note "Deposit cash"
```

Cross-currency move with explicit provider override:

```bash
bankero move 100 USD --from assets:wells-fargo --to assets:banesco 42000 VES @manual:420
```

**8.3 Buying goods/services**

Buy in local currency with official provider rate:

```bash
bankero buy external:traki 2500 VES --from assets:banesco @bcv --note "New clothes" --category expenses:clothes
```

Buy with intrinsic value tracked at market provider:

```bash
bankero buy external:farmatodo 840 VES --from assets:mercantil @bcv -b @binance --category expenses:health --tag pharmacy
```

Split a bill across categories:

```bash
bankero buy 500 USD --from assets:bank --to expenses:rent:450 --to expenses:water:50 --note "Monthly utilities"
```

**8.4 Selling a commodity (crypto-to-fiat exit)**

```bash
bankero sell 100 USDT --to assets:banesco 4500 VES @binance --note "P2P Exit" --tag crypto
```

**8.5 Revaluation (basis update without movement)**

```bash
bankero tag assets:gold-bar --set-basis @binance --note "Monthly revaluation"
```

**8.6 Budgets (monthly + effective balance)**

Create a monthly category budget and report it:

```bash
bankero budget create "Food" 300 USD --month 2026-02 --category expenses:food
bankero budget report --month 2026-02
```

Create an envelope budget tied to an account/provider:

```bash
bankero budget create "Materials" 1500 USDT @binance --account assets:personalbinance
```

Automate virtual siphoning (reserve money on matching credits):

```bash
bankero budget update "Materials" --auto-virtually-remove-from-balance --when every-credit --from income:mcdonalds --until 1300 USDT
```

Check actual vs reserved vs effective balance:

```bash
bankero balance assets:personalbinance
```

**8.6.1 Piggy banks (savings goals)**

```bash
bankero piggy create "New Car" 5000 USD --from assets:savings
bankero piggy status "New Car"
```

**8.6.2 Workspaces & projects**

```bash
bankero ws check
bankero ws add "Startup-X"
bankero ws checkout "Startup-X"

bankero project add "Fix roof"
bankero project checkout "Fix roof"
bankero project list
```

**8.7 Reporting**

```bash
bankero report --month 2026-02
bankero report --category expenses:food --month 2026-02
bankero report --tag groceries --month 2026-02
bankero report --account assets:banesco --range 2026-02-01..2026-02-29
```

**8.8 Multi-device sync**

```bash
bankero login
bankero sync status
bankero sync now
```

**8.9 Recurrent tasks & integrations**

```bash
bankero task create payoneer-sync --every 30m --webhook https://example.local/bankero/hooks/payoneer
bankero task enable payoneer-sync

bankero workflow runs --task payoneer-sync --last 10
bankero workflow events --run <run-id>
```

## 9. CLI requirements

### 9.1 Command grammar

The core action grammar is:

```bash
bankero <action> <amount> <commodity> --from <account> --to <account> [flags]
```

### 9.2 Actions

Bankero supports these user-facing actions:

- `deposit` — credit an asset account from an income/source account
- `move` — transfer value between accounts; may involve currency conversion
- `buy` — record a purchase (expense or external payee), optionally with provider/basis
- `sell` — record a sale/exchange, optionally with provider/basis
- `tag` — update reporting metadata (e.g., basis) without currency movement
- `budget` — manage monthly budgets
- `report` — query projections
- `balance` — show actual vs reserved vs effective balances
- `task` — manage recurrent tasks (schedule + webhook + enable/disable)
- `workflow` — inspect workflow runs and workflow events
- `login` — authenticate a device for sync
- `sync` — sync status and on-demand synchronization
- `ws` — manage workspaces (check/add/checkout)
- `project` — manage projects (add/checkout/list)
- `piggy` — manage piggy banks (create/status/fund)

### 9.3 Flags & tokens

- `--from <account>` (required when the action debits an account)
- `--to <account>` (repeatable for splits)
- `--note <text>` and `-m <text>`
- `--tag <name>` (repeatable)
- `--category <path>`
- `--month <YYYY-MM>`
- `--range <YYYY-MM-DD..YYYY-MM-DD>`
- `--account <account>`
- `--basis <amount commodity>` or `-b <amount commodity>`
- `-b @provider` or `--basis @provider`
- `@provider` and `@provider:rate`
- `--confirm`

### 9.3.1 Canonical subcommands (naming)

Bankero uses consistent resource-style naming for management commands:

- Budgets: `budget create`, `budget update`, `budget report`
- Balance: `balance [<account>]`
- Tasks: `task create`, `task update`, `task enable|disable`, `task run`, `task list`
- Sync: `login`, `sync status`, `sync now`
- Workspaces: `ws check`, `ws add`, `ws checkout`
- Projects: `project add`, `project checkout`, `project list`
- Piggy banks: `piggy create`, `piggy status`, `piggy fund`

### 9.4 Example commands (must be supported)

1) Income
```bash
bankero deposit 1500 USD --to assets:savings --from income:freelance -m "Web project payout"
```

2) Multi-currency move with manual rate
```bash
bankero move 100 USD --from assets:wells-fargo --to assets:banesco 42000 VES @manual:420
```

3) Buy with provider
```bash
bankero buy external:traki 2500 VES --from assets:banesco @bcv --note "New clothes"
```

4) Buy with provider + basis provider
```bash
bankero buy external:farmatodo 840 VES --from assets:mercantil @bcv -b @binance
```

5) Sell with provider
```bash
bankero sell 100 USDT --to assets:banesco 4500 VES @binance --note "P2P Exit"
```

6) Buy with explicit basis
```bash
bankero buy external:landlord 12000 VES --from assets:cash --basis 300 USD
```

7) Splits
```bash
bankero buy 500 USD --from assets:bank --to expenses:rent:450 --to expenses:water:50
```

8) Revaluation
```bash
bankero tag assets:gold-bar --set-basis @binance --note "Monthly revaluation"
```

9) Confirm mode
```bash
bankero move 5000 VES --from assets:wallet --to external:neighbor @binance --confirm
```

10) Liability
```bash
bankero buy assets:new-laptop 1200 USD --from liabilities:credit-card -b @binance
```

11) Create a virtual budget (envelope)
```bash
bankero budget create "Materials" 1500 USDT @binance --account assets:personalbinance
```

12) Create a monthly category budget and report it
```bash
bankero budget create "Food" 300 USD --month 2026-02 --category expenses:food
bankero budget report --month 2026-02
```

## 10. Functional requirements

### 10.1 Ledger correctness
- The system records every action as an immutable event.
- The system derives balances and reports from events only.
- The system produces deterministic results for the same event stream.

### 10.2 Multi-currency & conversions
- Conversions use an **as-of timestamp**.
- Rate selection is deterministic for a given provider and as-of timestamp.
- Manual overrides (`@provider:rate`) are recorded in the event.

### 10.3 Intrinsic value (basis)
- `--basis/-b` can be:
  - A fixed amount+commodity, or
  - A provider token (`@provider`) meaning “compute basis using that provider”.
- Basis is preserved as event metadata and is queryable in reports.

### 10.4 Tags & categories
- Events can be labeled with:
  - Zero or more tags
  - A category path
- Reports can be filtered/aggregated by tag/category.

### 10.5 Monthly budgets
- Users set a budget per (month, category) with a target commodity.
- Reports show actual vs budget and variance.

### 10.5.1 Virtual budget deficits (effective balance)
- Budgets reserve money as a virtual deficit without creating real ledger movements.
- Balances expose both:
  - **Actual balance** (derived from ledger postings)
  - **Reserved** amounts (budgets/piggy allocations)
  - **Effective total** = actual + reserved adjustments

### 10.5.2 Budget automation (virtual siphoning)
- A budget can define rules that reserve money virtually when matching credits occur (e.g., every credit from `income:mcdonalds` until a threshold is reached).

### 10.6 Reports
- Reports support at least:
  - By month
  - By date range
  - By account prefix
  - By category
  - By tag
  - By commodity
- Reports are reproducible for a given event stream.

### 10.7 Sync (multi-device)
- Sync never deletes or overwrites events.
- Sync resolves conflicts by merging event streams (not state) and rebuilding projections.
- The sync server authenticates devices and authorizes ledger access.
- The sync server can emit webhooks based on event patterns.

### 10.8 Recurrent tasks & workflow execution
- The system supports creating recurrent tasks with: `task_id`, name, schedule, webhook target, enabled/disabled state.
- Each task execution creates a workflow run with a unique `run_id`.
- Workflows emit workflow events (for observability and audit) and may append ledger events.
- Workflow-driven ledger events are validated by the same domain invariants as user-entered CLI events.
- A workflow may append additional workflow events to compose multi-step pipelines (Zapier/n8n-style), while keeping the final ledger append deterministic.

## 11. Non-functional requirements

- **Offline-first**: all core actions function without network.
- **Auditability**: events are immutable; projections are rebuildable.
- **Determinism**: identical event inputs produce identical projections.
- **Performance**: common reports complete quickly on typical laptop hardware.
- **Security**:
  - Device login flow does not embed credentials in the CLI.
  - Tokens/keys are stored locally with minimal exposure.
- **Portability**: ledger can be backed up and restored easily.

## 11.1 Stack (local persistence + sync)

Bankero keeps the CLI lightweight while providing robust local persistence and seamless multi-device sync.

- **SQLite (local persistence)**
  - SQLite is the local source of truth for persistence.
  - It stores the immutable event journal and rebuildable projections.

- **Loro (conflict-free state layer)**
  - Loro represents and replicates shared ledger state structures across devices.
  - Loro provides conflict-free convergence for replicated data; the accounting domain enforces ledger invariants and deterministic math.

- **Iroh (seamless sync transport)**
  - Iroh is the sync transport layer used to exchange updates between devices.
  - Iroh’s local discovery (mDNS on Wi‑Fi) provides “it just works” peer discovery while still supporting remote relay.

This stack meets the PRD’s sync requirements: no last-write-wins, no lost events, and deterministic convergence across many offline-capable devices.

## 11.2 “Lightweight & resilient” backend stack (cloud peer + backup)

Bankero treats the **local network as the primary sync layer** and the **cloud as a delayed, batched backup**. This handles intermittent internet drops cleanly: devices converge on Wi‑Fi quickly, and push compacted state to the cloud when reachable.

### Backend code

- **App server**: Rust + Axum
  - Fast and type-safe.
  - Shares the same replicated data model as the CLI (so encoding/decoding is consistent).

- **Sync node**: Iroh (embedded in the Axum service)
  - Acts as an always-on cloud peer.
  - Provides relay behavior when devices can’t connect directly.

- **Database**: Postgres
  - Stores finalized, batched replicated-state blobs (compacted Loro binary payloads).
  - Supports audit/retention policies and server-side webhook triggers.

### Local testing & replication

- **Docker Compose**
  - A single `docker-compose.yml` spins up Postgres + the Axum backend in isolated, reproducible containers.

### Production deployment

- **Fly.io**
  - Lightweight infrastructure-as-code using `fly.toml`.
  - Supports raw **UDP routing**, which Iroh uses for efficient P2P hole punching; many platforms block UDP.

### Sync execution model

- **Wi‑Fi first**: Iroh’s mDNS discovery connects local devices instantly when they share a network.
- **Batched cloud push**: when a device detects the cloud peer (via Iroh relay), it uploads a single compacted Loro binary blob, reducing bandwidth and handling long offline periods gracefully.

## 12. Data & event requirements (schema-level, implementation-agnostic)

Each event MUST include:

- `event_id` (globally unique)
- `ledger_id`
- `workspace_id`
- `project_id` (nullable)
- `device_id`
- `created_at` (device time)
- `effective_at` (financial time for ordering/reporting)
- `action` (deposit/move/buy/sell/tag/budget)
- `postings` / legs:
  - accounts affected
  - amounts + commodities
- `rate_context`:
  - provider token(s)
  - optional override value
  - as-of timestamp used
- `basis` context:
  - fixed value OR provider reference
- `tags[]`, `category`, `note`

Each workflow run MUST include:

- `run_id` (globally unique)
- `task_id` (nullable for manual runs)
- `ledger_id`, `device_id`
- `started_at`, `finished_at`
- `webhook_target`
- `workflow_events[]` (append-only)

Each workflow event SHOULD include:

- `run_id`
- `event_id` (unique within run)
- `type` (e.g., `WebhookCalled`, `WebhookResponseReceived`, `TransformApplied`, `LedgerEventProposed`, `LedgerEventCommitted`, `Error`)
- `timestamp`
- `payload` (structured)

Projections MUST be rebuildable from events.

## 13. Acceptance criteria

Implementation status (as of 2026-02-25):

- [ ] All example commands in section 9.4 produce a stored immutable event.
  - Notes: most core examples are supported; `move ... @provider --confirm` resolves rates deterministically from the offline rate store (and prints a value preview). Remaining gaps are mostly around a full provider-backed conversion engine and provider-backed basis computation.
- [ ] After syncing two devices that created events offline, the merged ledger contains both events and projections match on both devices.
- [x] Reports filtered by `--tag` and `--category` return consistent results.
- [ ] Budget report shows budget vs actual for a given month.
- [ ] `bankero balance` shows actual balance, reserved (virtual deficits), and effective total.
  - Notes: current `balance` shows actual (sum of postings) only.
- [x] Rebuilding projections from events yields the same balances and reports.
- [x] Switching workspaces isolates data completely (events and projections do not bleed across).
- [x] Checking out a project auto-tags subsequent actions to that project.
  - Notes: project is stored on each event payload; project listing/spend rollups are not implemented yet.
- [ ] Piggy banks report progress deterministically for a given event stream.
- [ ] Creating a recurrent task and running it produces a workflow run id and an auditable stream of workflow events.
- [ ] A workflow that imports Payoneer transactions appends deterministic ledger events (no duplicates, no lost events) and remains safe under multi-device sync.

## 14. Open questions

- How is `effective_at` chosen by default for each action (now vs explicit flag)?
- Are categories strictly hierarchical and validated, or free-form strings?
- Are tags free-form or pre-declared?
- What is the canonical “reference commodity” for basis reporting (user-configurable)?
- How are fees/spreads represented for provider conversions?

## 15. Milestones

Milestone tracking (as of 2026-02-25):

- [x] 1. Event journal + projections framework
  - [x] SQLite-backed immutable event journal
  - [x] Deterministic read models by replaying events

- [ ] 2. Actions: `deposit`, `move`, `buy`, `sell` + `--confirm` flows
  - [x] Core actions write immutable events: `deposit`, `move`, `buy`, `sell`, `tag`
  - [ ] `--confirm` flow matches PRD intent (rate fetch/preview + confirmation)
    - [x] `move ... @provider --confirm` resolves provider rates deterministically and prints a value preview
    - [x] Deterministic provider rate resolution (offline rate store; no interactive prompt)
    - [x] Basis-provider computation in `--confirm` using stored rates (`-b @provider`)
  - [x] CLI grammar matches section 9.4 for `buy` split form (no payee)

- [ ] 3. Providers + overrides (`@provider`, `@provider:rate`) + time-based conversion
  - [x] Offline rate store (`bankero rate set|get|list`) and deterministic resolution for `--confirm` previews
  - [x] Computed cross-currency `move` form using stored rates (`move 100 USD ... VES @provider`)
  - [ ] Full provider-backed conversion engine (compute missing legs, deterministic provider logic)

- [ ] 4. Basis (`--basis/-b`) with provider computation
  - [x] Deterministic basis computation from stored provider rates in `--confirm` for `-b @provider`

- [ ] 5. Tags/categories + filtering
  - [x] Tags/categories stored on events and filterable in `report`

- [ ] 6. Monthly budgets + budget reports

- [ ] 7. Multi-device sync + device login + webhook rules
