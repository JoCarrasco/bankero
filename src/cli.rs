use crate::domain::ProviderToken;
use clap::{Args, Parser, Subcommand};
use rust_decimal::Decimal;

#[derive(Debug, Parser)]
#[command(name = "bankero")]
#[command(
    about = "Local-first multi-currency ledger",
    long_about = None,
    version,
    propagate_version = true,
    infer_long_args = true
)]
pub struct Cli {
    /// Override Bankero home directory (config/data subdirs will be created inside it).
    #[arg(long, env = "BANKERO_HOME")]
    pub home: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(
        about = "Record a deposit between two accounts",
        long_about = r#"Record a deposit between two accounts.

This writes a single transaction event with two postings:
- decreases the --from account
- increases the --to account

Examples:
    bankero deposit 1200 USD --from assets:cash --to income:salary
    bankero deposit 50 EUR --from assets:cash --to income:gifts --tag bonus
"#
    )]
    Deposit(DepositArgs),

    #[command(
        about = "Move value between accounts (optionally cross-currency)",
        long_about = r#"Move value between accounts.

Same-currency move:
    bankero move 25 USD --from assets:cash --to expenses:food

Cross-currency move:
    bankero move 100 USD --from assets:usd --to assets:ves 3600 VES

Cross-currency move (compute quote amount from stored provider rate):
    bankero rate set @bcv USD VES 45.2 --as-of 2026-02-25T12:00:00Z
    bankero move 100 USD --from assets:usd --to assets:ves VES @bcv

Provider context (used in --confirm preview for value/rate):
    bankero move 100 USD --from assets:usd --to assets:ves 3600 VES @binance --confirm
"#
    )]
    Move(MoveArgs),

    #[command(
        about = "Record a buy (payee form or split form)",
        long_about = r#"Record a buy.

Two supported forms:

1) Payee form (3-arg):
    bankero buy <payee> <amount> <commodity> --from <account>

2) Split form (2-arg):
    bankero buy <amount> <commodity> --from <account> --to <account:amount> [--to ...]

Examples:
    bankero buy merchant:amazon 39.99 USD --from assets:cash
    bankero buy 100 USD --from assets:cash --to expenses:food:60 --to expenses:transport:40

Provider context (used in --confirm preview for value/rate):
    bankero buy 100 USD --from assets:cash --to expenses:food:100 @bcv --confirm
"#
    )]
    Buy(BuyArgs),

    #[command(
        about = "Record a sell (cross-currency with explicit quote amount)",
        long_about = r#"Record a sell.

This is the explicit cross-currency form: you provide both the base amount
(what you're selling) and the quote amount (what you receive).

Example:
    bankero sell 0.01 BTC --to assets:cash 2400 USD @binance

Tip:
    Use --confirm to preview the computed value and optionally provide rates via a provider.
"#
    )]
    Sell(SellArgs),

    #[command(
        about = "Tag an account or asset (optionally set basis)",
        long_about = r#"Tag an account/asset.

Use this to attach tags and/or set basis (intrinsic value) metadata.

Examples:
    bankero tag assets:gold-bar --tag longterm
    bankero tag assets:gold-bar --set-basis "2000 USD" --confirm
    bankero tag assets:btc --set-basis "@binance" --confirm
"#
    )]
    Tag(TagArgs),

    #[command(
        about = "Show balances",
        long_about = r#"Show balances.

By default prints balances for all accounts. If you pass an account prefix,
filters the output to that subtree.

Examples:
    bankero balance
    bankero balance assets
    bankero balance assets:cash
"#
    )]
    Balance(BalanceArgs),

    #[command(
        about = "Generate a report (filters by time/account/category/tag/commodity)",
        long_about = r#"Generate a report.

Reports are derived by replaying the journal and then applying filters.

Time filters:
    --month YYYY-MM
    --range YYYY-MM-DD..YYYY-MM-DD

Other filters:
    --account <account-prefix>
    --category <category>
    --tag <tag>
    --commodity <commodity>

Examples:
    bankero report --month 2026-02
    bankero report --range 2026-02-01..2026-02-15 --account expenses
    bankero report --month 2026-02 --category income:freelance
"#
    )]
    Report(ReportArgs),

    #[command(
        about = "Manage offline provider FX rates",
        long_about = r#"Manage offline provider FX rates.

Bankero is offline-first, so provider rates must be available locally for
deterministic previews/conversions.

Examples:
    bankero rate set @binance USD VES 45.2 --as-of 2026-02-25T12:00:00Z
    bankero rate get @binance USD VES --as-of 2026-02-25T12:00:00Z
    bankero rate list @binance USD VES
"#
    )]
    Rate(RateArgs),

    #[command(
        about = "Budget commands (stub)",
        long_about = r#"Budget commands (stub).

This command group is reserved for later milestones.
"#
    )]
    Budget(BudgetArgs),

    #[command(
        about = "Workspace management",
        long_about = r#"Workspace management.

Workspaces are isolated ledgers. Switching workspaces changes which SQLite journal
Bankero writes to.

Examples:
    bankero ws check
    bankero ws add personal
    bankero ws checkout personal
"#
    )]
    Ws(WsArgs),

    #[command(
        about = "Project management within a workspace",
        long_about = r#"Project management.

Projects let you group activity inside a workspace.

Examples:
    bankero project list
    bankero project add side-hustle
    bankero project checkout side-hustle
"#
    )]
    Project(ProjectArgs),

    // Stubs for later milestones
    #[command(about = "Task commands (stub)", long_about = "Task commands (stub).")]
    Task(TaskArgs),

    #[command(
        about = "Workflow commands (stub)",
        long_about = "Workflow commands (stub)."
    )]
    Workflow(WorkflowArgs),

    #[command(
        about = "Login (stub)",
        long_about = "Login is a stub for later milestones."
    )]
    Login,

    #[command(
        about = "Sync commands (stub)",
        long_about = "Sync commands are a stub for later milestones."
    )]
    Sync(SyncArgs),

    #[command(
        about = "Piggy commands (stub)",
        long_about = "Piggy commands are a stub for later milestones."
    )]
    Piggy(PiggyArgs),
}

#[derive(Debug, Args)]
pub struct RateArgs {
    #[command(subcommand)]
    pub command: RateCommand,
}

#[derive(Debug, Subcommand)]
pub enum RateCommand {
    #[command(
        about = "Set a provider rate at an as-of timestamp",
        long_about = r#"Set a provider rate.

The rate is interpreted as: <quote> per <base>.

Example:
    bankero rate set @bcv USD VES 45.2 --as-of 2026-02-25T12:00:00Z
"#
    )]
    Set(RateSetArgs),

    #[command(
        about = "Get the latest provider rate at or before as-of",
        long_about = r#"Get a provider rate.

Returns the most recent stored rate at or before the provided --as-of timestamp.

Example:
    bankero rate get @bcv USD VES --as-of 2026-02-25T12:00:00Z
"#
    )]
    Get(RateGetArgs),

    #[command(
        about = "List stored rates (newest first)",
        long_about = r#"List stored rates (newest first).

Example:
    bankero rate list @bcv USD VES
"#
    )]
    List(RateListArgs),
}

#[derive(Debug, Args, Clone)]
pub struct CommonEventFlags {
    #[arg(long, short = 'm', alias = "note")]
    pub note: Option<String>,

    #[arg(long = "tag")]
    pub tags: Vec<String>,

    #[arg(long)]
    pub category: Option<String>,

    /// Asks for confirmation before writing an event.
    #[arg(
        long,
        long_help = r#"Ask for confirmation before writing an event.

In confirm mode Bankero may prompt you for additional information (like an FX rate)
and will print a preview (e.g., transaction value) before it writes to the journal.
"#
    )]
    pub confirm: bool,

    /// Financial time for ordering/reporting (RFC3339). Defaults to now.
    #[arg(
        long,
        long_help = r#"Financial time for ordering/reporting (RFC3339).

Defaults to now.
Example:
    --effective-at 2026-02-25T10:30:00Z
"#
    )]
    pub effective_at: Option<String>,

    /// As-of timestamp for rate resolution (RFC3339). Defaults to effective_at.
    #[arg(
        long,
        long_help = r#"As-of timestamp for rate resolution (RFC3339).

Defaults to effective_at.
"#
    )]
    pub as_of: Option<String>,

    /// Basis (intrinsic value) as either fixed "<amount> <commodity>" (use --basis-amount/--basis-commodity) or provider token like "@binance".
    #[arg(
        long,
        short = 'b',
        long_help = r#"Basis (intrinsic value) for an asset.

Accepts either:
- fixed basis: "<amount> <commodity>" (example: --basis "2000 USD")
- provider token: "@provider" (example: --basis "@binance")

In confirm mode, provider basis can prompt you to materialize the basis amount.
"#
    )]
    pub basis: Option<String>,
}

#[derive(Debug, Args)]
pub struct RateSetArgs {
    /// Provider token like "@binance" (the leading '@' is optional).
    pub provider: String,
    pub base: String,
    pub quote: String,
    pub rate: Decimal,

    /// As-of timestamp (RFC3339). Defaults to now.
    #[arg(long)]
    pub as_of: Option<String>,
}

#[derive(Debug, Args)]
pub struct RateGetArgs {
    /// Provider token like "@binance" (the leading '@' is optional).
    pub provider: String,
    pub base: String,
    pub quote: String,

    /// As-of timestamp (RFC3339). Defaults to now.
    #[arg(long)]
    pub as_of: Option<String>,
}

#[derive(Debug, Args)]
pub struct RateListArgs {
    /// Provider token like "@binance" (the leading '@' is optional).
    pub provider: String,
    pub base: String,
    pub quote: String,
}

#[derive(Debug, Args)]
#[command(
    about = "Deposit: move value between two accounts",
    long_about = r#"Deposit command.

Writes a journal event that credits the destination account and debits the source.

Example:
    bankero deposit 1200 USD --from assets:cash --to income:salary
"#
)]
pub struct DepositArgs {
    pub amount: String,
    pub commodity: String,

    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,

    #[command(flatten)]
    pub common: CommonEventFlags,
}

#[derive(Debug, Args)]
#[command(
    about = "Move: transfer value between accounts",
    long_about = r#"Move command.

Same-currency:
    bankero move 25 USD --from assets:cash --to expenses:food

Cross-currency (provide quote amount + commodity):
    bankero move 100 USD --from assets:usd --to assets:ves 3600 VES

Provider context:
    bankero move 100 USD --from assets:usd --to assets:ves 3600 VES @binance --confirm
"#
)]
pub struct MoveArgs {
    pub amount: String,
    pub commodity: String,

    #[arg(long)]
    pub from: String,

    #[arg(long)]
    pub to: String,

    #[command(flatten)]
    pub common: CommonEventFlags,

    /// Optional tail supporting same- or cross-currency moves.
    ///
    /// Supported forms:
    /// - same-currency: (no tail)
    /// - same-currency with provider context: `@provider` or `@provider:rate`
    /// - cross-currency (explicit quote): `<to_amount> <to_commodity> [@provider[:rate]]`
    /// - cross-currency (computed quote): `<to_commodity> @provider[:rate]`
    #[arg(num_args = 0..=3)]
    pub tail: Vec<String>,
}

#[derive(Debug, Args)]
#[command(
    about = "Buy: record a purchase",
    long_about = r#"Buy command.

Payee form (3 args):
    bankero buy <payee> <amount> <commodity> --from <account>

Split form (2 args):
    bankero buy <amount> <commodity> --from <account> --to <account:amount> [--to ...]
"#
)]
pub struct BuyArgs {
    /// Either a payee/target account (3-arg form) OR the amount (2-arg split form).
    ///
    /// Supported forms:
    /// - `bankero buy <payee> <amount> <commodity> --from ...`
    /// - `bankero buy <amount> <commodity> --from ... --to <account:amount> [--to ...]`
    pub payee_or_amount: String,

    /// Either the amount (3-arg form) OR the commodity (2-arg split form).
    pub amount_or_commodity: String,

    /// Present only in the 3-arg form.
    pub commodity: Option<String>,

    #[arg(long)]
    pub from: String,

    /// Optional splits like "expenses:rent:450" (account + amount).
    #[arg(long = "to")]
    pub to_splits: Vec<String>,

    #[command(flatten)]
    pub common: CommonEventFlags,

    /// Optional provider token like "@bcv".
    pub provider: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    about = "Sell: record a sale",
    long_about = r#"Sell command.

Provide the base amount (what you sell) and the quote amount/commodity (what you receive).

Example:
    bankero sell 0.01 BTC --to assets:cash 2400 USD @binance
"#
)]
pub struct SellArgs {
    pub amount: String,
    pub commodity: String,

    #[arg(long)]
    pub from: Option<String>,

    #[arg(long)]
    pub to: String,

    #[command(flatten)]
    pub common: CommonEventFlags,

    /// Required quote amount (e.g., the VES received).
    pub to_amount: Decimal,

    /// Required quote commodity (e.g., VES).
    pub to_commodity: String,

    /// Optional provider token like "@binance".
    pub provider: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    about = "Tag: attach metadata to an account/asset",
    long_about = r#"Tag command.

Use --tag to add tags and/or --set-basis to record intrinsic value metadata.

Note: provider-based basis computation (e.g. "@binance") requires a movement event
with an outgoing posting (like `buy`/`sell`/`move --confirm`). For `tag`, use a
fixed basis like "2000 USD".
"#
)]
pub struct TagArgs {
    /// Target account or asset to tag (e.g., assets:gold-bar)
    pub target: String,

    /// Update intrinsic value without movement
    #[arg(long = "set-basis")]
    pub set_basis: Option<String>,

    #[command(flatten)]
    pub common: CommonEventFlags,
}

#[derive(Debug, Args)]
#[command(
    about = "Balance: show balances",
    long_about = r#"Balance command.

Examples:
    bankero balance
    bankero balance assets
    bankero balance assets --month 2026-02
"#
)]
pub struct BalanceArgs {
    /// Optional month context used for budget reservations (YYYY-MM).
    #[arg(long)]
    pub month: Option<String>,

    pub account: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    about = "Report: list events and totals (filtered)",
    long_about = r#"Report command.

Examples:
    bankero report --month 2026-02
    bankero report --range 2026-02-01..2026-02-15 --account expenses
"#
)]
pub struct ReportArgs {
    #[arg(long)]
    pub month: Option<String>,

    #[arg(long)]
    pub range: Option<String>,

    #[arg(long)]
    pub account: Option<String>,

    #[arg(long)]
    pub category: Option<String>,

    #[arg(long)]
    pub tag: Option<String>,

    #[arg(long)]
    pub commodity: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum WsCmd {
    #[command(
        about = "Show the current workspace",
        long_about = r#"Show the current workspace.

Prints the workspace name currently selected in your Bankero config.
"#
    )]
    Check,

    #[command(
        about = "Add a new workspace",
        long_about = r#"Add a new workspace.

Creates workspace metadata and makes it available for checkout.
"#
    )]
    Add { name: String },

    #[command(
        about = "Switch to an existing workspace",
        long_about = r#"Switch to an existing workspace.

After checkout, new events are written to that workspace's journal.
"#
    )]
    Checkout { name: String },
}

#[derive(Debug, Args)]
pub struct WsArgs {
    #[command(subcommand)]
    pub cmd: WsCmd,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCmd {
    #[command(about = "Add a new project", long_about = "Add a new project.")]
    Add { name: String },

    #[command(
        about = "Switch to an existing project",
        long_about = "Switch to an existing project."
    )]
    Checkout { name: String },

    #[command(about = "List known projects", long_about = "List known projects.")]
    List,
}

#[derive(Debug, Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub cmd: ProjectCmd,
}

#[derive(Debug, Subcommand)]
pub enum BudgetCmd {
    #[command(about = "Create a budget", long_about = "Create a budget.")]
    Create {
        name: String,
        amount: String,
        commodity: String,
        #[arg(long)]
        month: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(trailing_var_arg = true)]
        extra: Vec<String>,
    },

    #[command(
        about = "Update an existing budget",
        long_about = r#"Update an existing budget.

This milestone supports budget automation (virtual siphoning): reserve money
virtually when matching credits happen.

Examples:
    bankero budget update "Food" --auto-reserve-from income:salary --until 200 USD
    bankero budget update "Food" --clear-auto-reserve
"#
    )]
    Update {
        name: String,

        /// Enable auto-reserve (virtual siphoning) when credits come from this account prefix.
        #[arg(long = "auto-reserve-from")]
        auto_reserve_from: Option<String>,

        /// Cap the total reserved amount for the month.
        #[arg(long, value_names = ["AMOUNT", "COMMODITY"], num_args = 2)]
        until: Option<Vec<String>>,

        /// Disable auto-reserve automation for this budget.
        #[arg(long = "clear-auto-reserve")]
        clear_auto_reserve: bool,
    },

    #[command(about = "Show a budget report", long_about = "Show a budget report.")]
    Report {
        #[arg(long)]
        month: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct BudgetArgs {
    #[command(subcommand)]
    pub cmd: BudgetCmd,
}

#[derive(Debug, Subcommand)]
pub enum SyncCmd {
    #[command(about = "Show sync status", long_about = "Show sync status.")]
    Status,

    #[command(about = "Run a sync now", long_about = "Run a sync now.")]
    Now,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub cmd: SyncCmd,
}

#[derive(Debug, Subcommand)]
pub enum TaskCmd {
    #[command(about = "Create a task", long_about = "Create a task.")]
    Create { task_id: String },

    #[command(about = "Update a task", long_about = "Update a task.")]
    Update { task_id: String },

    #[command(about = "Enable a task", long_about = "Enable a task.")]
    Enable { task_id: String },

    #[command(about = "Disable a task", long_about = "Disable a task.")]
    Disable { task_id: String },

    #[command(about = "Run a task", long_about = "Run a task.")]
    Run { task_id: String },

    #[command(about = "List tasks", long_about = "List tasks.")]
    List,
}

#[derive(Debug, Args)]
pub struct TaskArgs {
    #[command(subcommand)]
    pub cmd: TaskCmd,
}

#[derive(Debug, Subcommand)]
pub enum WorkflowCmd {
    #[command(
        about = "List recent workflow runs",
        long_about = "List recent workflow runs."
    )]
    Runs {
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        last: Option<u32>,
    },

    #[command(
        about = "List workflow events for a given run",
        long_about = "List workflow events for a given run."
    )]
    Events {
        #[arg(long)]
        run: String,
    },
}

#[derive(Debug, Args)]
pub struct WorkflowArgs {
    #[command(subcommand)]
    pub cmd: WorkflowCmd,
}

#[derive(Debug, Subcommand)]
pub enum PiggyCmd {
    #[command(about = "Create a new piggy", long_about = "Create a new piggy.")]
    Create {
        name: String,
        amount: String,
        commodity: String,
        #[arg(long)]
        from: String,
    },

    #[command(about = "Show piggy status", long_about = "Show piggy status.")]
    Status { name: String },

    #[command(about = "Fund a piggy", long_about = "Fund a piggy.")]
    Fund { name: String },
}

#[derive(Debug, Args)]
pub struct PiggyArgs {
    #[command(subcommand)]
    pub cmd: PiggyCmd,
}

pub fn parse_provider_opt(raw: &Option<String>) -> Option<ProviderToken> {
    raw.as_deref().and_then(crate::domain::parse_provider_token)
}
