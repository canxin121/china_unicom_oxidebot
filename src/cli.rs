use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum AccountCommand {
    /// Add an account and import its canonical four-field login JSON
    Add {
        /// Stable account identifier (letters, digits, `_` and `-`)
        account_id: String,
        /// Friendly display name; defaults to account_id
        #[arg(long)]
        name: Option<String>,
    },
    /// Replace an account's credentials with a new four-field login JSON
    Login { account_id: String },
    /// List all China Unicom accounts owned by this bot user
    List,
    /// Remove one account and its usage history
    Remove { account_id: String },
}

#[derive(Subcommand)]
pub enum TaskCommand {
    /// Start one account, or every account when account_id is omitted
    Start { account_id: Option<String> },
    /// Stop one account, or every account when account_id is omitted
    Stop { account_id: Option<String> },
    /// Show one account, or every account when account_id is omitted
    Status { account_id: Option<String> },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show one account, or every account when account_id is omitted
    Show { account_id: Option<String> },
    /// Interactively change one account
    Set { account_id: String },
}

#[derive(Subcommand)]
pub enum Commands {
    /// Add, log in, list, or remove China Unicom accounts
    #[command(short_flag = 'a')]
    Account {
        #[command(subcommand)]
        account_command: AccountCommand,
    },
    /// Query one account, or every account when account_id is omitted
    #[command(short_flag = 'q')]
    Query { account_id: Option<String> },
    /// Show or change per-account configuration
    #[command(short_flag = 'c')]
    Config {
        #[command(subcommand)]
        config_command: ConfigCommand,
    },
    /// Check or control per-account scheduled tasks
    #[command(short_flag = 't')]
    Task {
        #[command(subcommand)]
        task_command: TaskCommand,
    },
}

#[derive(Parser)]
#[command(
    name = "/china_unicom",
    version = env!("CARGO_PKG_VERSION"),
    author = "canxin121",
    about = "Manage multiple China Unicom accounts using canonical four-field login JSON."
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

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{AccountCommand, Cli, Commands, TaskCommand};

    #[test]
    fn parses_multi_account_commands() {
        let cli =
            Cli::try_parse_from(["/china_unicom", "account", "add", "main", "--name", "主卡"])
                .unwrap();
        assert!(matches!(
            cli.command,
            Commands::Account {
                account_command: AccountCommand::Add { account_id, name }
            } if account_id == "main" && name.as_deref() == Some("主卡")
        ));

        let cli = Cli::try_parse_from(["/china_unicom", "query"]).unwrap();
        assert!(matches!(cli.command, Commands::Query { account_id: None }));

        let cli = Cli::try_parse_from(["/china_unicom", "task", "status", "backup"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Task {
                task_command: TaskCommand::Status { account_id: Some(id) }
            } if id == "backup"
        ));
    }
}
