use crate::domain::ProviderToken;
use clap::{Args, Parser, Subcommand};

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

    #[arg(trailing_var_arg = true)]
    pub extra: Vec<String>,
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

    /// Trailing args to support: <to_amount> <to_commodity> @provider or @provider alone
    #[arg(trailing_var_arg = true)]
    pub extra: Vec<String>,
}

#[derive(Debug, Args)]
pub struct BuyArgs {
    /// Payee or target account (e.g., external:traki, assets:new-laptop)
    pub payee: String,

    pub amount: String,
    pub commodity: String,

    #[arg(long)]
    pub from: String,

    /// Optional splits like "expenses:rent:450" (account + amount).
    #[arg(long = "to")]
    pub to_splits: Vec<String>,

    #[command(flatten)]
    pub common: CommonEventFlags,

    /// Trailing args to support provider tokens like @bcv
    #[arg(trailing_var_arg = true)]
    pub extra: Vec<String>,
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

    /// Trailing args to support: <to_amount> <to_commodity> @provider
    #[arg(trailing_var_arg = true)]
    pub extra: Vec<String>,
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

pub fn find_provider_token(extra: &[String]) -> Option<ProviderToken> {
    for token in extra {
        if let Some(p) = crate::domain::parse_provider_token(token) {
            return Some(p);
        }
    }
    None
}
