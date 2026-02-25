use crate::domain::ProviderToken;
use clap::{Args, Parser, Subcommand};
use rust_decimal::Decimal;

#[derive(Debug, Parser)]
#[command(name = "bankero")]
#[command(about = "Local-first multi-currency ledger", long_about = None)]
pub struct Cli {
    /// Override Bankero home directory (config/data subdirs will be created inside it).
    #[arg(long, env = "BANKERO_HOME")]
    pub home: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Deposit(DepositArgs),
    Move(MoveArgs),
    Buy(BuyArgs),
    Sell(SellArgs),
    Tag(TagArgs),

    Balance(BalanceArgs),
    Report(ReportArgs),

    Budget(BudgetArgs),
    Ws(WsArgs),
    Project(ProjectArgs),

    // Stubs for later milestones
    Task(TaskArgs),
    Workflow(WorkflowArgs),
    Login,
    Sync(SyncArgs),
    Piggy(PiggyArgs),
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
    #[arg(long)]
    pub confirm: bool,

    /// Financial time for ordering/reporting (RFC3339). Defaults to now.
    #[arg(long)]
    pub effective_at: Option<String>,

    /// As-of timestamp for rate resolution (RFC3339). Defaults to effective_at.
    #[arg(long)]
    pub as_of: Option<String>,

    /// Basis (intrinsic value) as either fixed "<amount> <commodity>" (use --basis-amount/--basis-commodity) or provider token like "@binance".
    #[arg(long, short = 'b')]
    pub basis: Option<String>,
}

#[derive(Debug, Args)]
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
    /// - cross-currency: `<to_amount> <to_commodity> [@provider[:rate]]`
    #[arg(num_args = 0..=3)]
    pub tail: Vec<String>,
}

#[derive(Debug, Args)]
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
pub struct BalanceArgs {
    pub account: Option<String>,
}

#[derive(Debug, Args)]
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
    Check,
    Add { name: String },
    Checkout { name: String },
}

#[derive(Debug, Args)]
pub struct WsArgs {
    #[command(subcommand)]
    pub cmd: WsCmd,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCmd {
    Add { name: String },
    Checkout { name: String },
    List,
}

#[derive(Debug, Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub cmd: ProjectCmd,
}

#[derive(Debug, Subcommand)]
pub enum BudgetCmd {
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
    Update { name: String },
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
    Status,
    Now,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub cmd: SyncCmd,
}

#[derive(Debug, Subcommand)]
pub enum TaskCmd {
    Create { task_id: String },
    Update { task_id: String },
    Enable { task_id: String },
    Disable { task_id: String },
    Run { task_id: String },
    List,
}

#[derive(Debug, Args)]
pub struct TaskArgs {
    #[command(subcommand)]
    pub cmd: TaskCmd,
}

#[derive(Debug, Subcommand)]
pub enum WorkflowCmd {
    Runs {
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        last: Option<u32>,
    },
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
    Create {
        name: String,
        amount: String,
        commodity: String,
        #[arg(long)]
        from: String,
    },
    Status { name: String },
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
