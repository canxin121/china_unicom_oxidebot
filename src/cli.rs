use clap::{Parser, Subcommand};

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
    Set,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Register to China Unicom Oxidebot service
    #[command(short_flag = 'r')]
    Register,

    /// Deregister from China Unicom Oxidebot service
    #[command(short_flag = 'd')]
    Deregister,

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
