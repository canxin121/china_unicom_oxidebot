use clap::{builder::Str, Parser, Subcommand};

#[derive(Subcommand)]
pub enum TaskCommand {
    /// Start the task
    Start,
    /// Stop the task
    Stop,
    /// Check the status of the task
    Status,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show configuration
    Show,

    /// Set configuration, use 'None' to set null
    Set {
        /// String?: Cookie for login, should contain 'ecs_token' and 'ecs_acc'
        #[arg(short, long)]
        cookie: Option<String>,

        /// Int?: Interval time for task execution in seconds
        #[arg(short, long)]
        interval: Option<i64>,

        /// Int?: After this time is exceeded, a notification is sent even if the threshold is not exceeded
        #[arg(short, long)]
        timeout: Option<String>,

        /// Float?: Free flow threshold, send notification if exceeded
        #[arg(short, long)]
        free_threshold: Option<String>,

        /// Float?: Nonfree flow threshold, send notification if exceeded
        #[arg(short, long)]
        nonfree_threshold: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum Commands {
    /// Register to China Unicom Oxidebot service
    #[command(short_flag = 'r')]
    Register {
        /// String: Cookie for login
        #[arg(short, long)]
        cookie: String,
        #[arg(short, long)]
        app_id: String,
        #[arg(short, long)]
        token_online: String,
    },

    /// Deregister from China Unicom Oxidebot service
    #[command(short_flag = 'd')]
    Deregister {
        #[arg(short = 'y')]
        yes: bool,
    },

    /// Show or set configuration
    #[command(short_flag = 'c')]
    Config {
        #[command(subcommand)]
        config_command: ConfigCommand,
    },

    /// Query data immediately
    #[command(short_flag = 'q')]
    Query,

    /// Check or control task
    #[command(short_flag = 't')]
    Task {
        #[command(subcommand)]
        task_command: TaskCommand,
    },
}

#[derive(Parser)]
#[command(
    name = "/china_unicom",
    version = "0.1.0",
    author = "canxin121",
    about = "A chatbot cli to actively check or receive scheduled/threshold notifications about China Unicom flow usage."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub fn name() -> &'static str {
        "/china_unicom"
    }
}
