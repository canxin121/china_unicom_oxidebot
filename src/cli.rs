//! Typed `/china_unicom` command grammar.

use oxidebot::prelude::{BotCommand, CommandArgs};

/// Adds one China Unicom account.
#[derive(Debug, CommandArgs)]
pub struct AccountAddArgs {
    /// Stable account identifier made of letters, digits, `_`, and `-`.
    pub account_id: String,
    /// Optional friendly display name. Defaults to `account_id`.
    #[arg(long)]
    pub name: Option<String>,
}

/// Selects one account by its stable identifier.
#[derive(Debug, CommandArgs)]
pub struct AccountIdArgs {
    /// Stable account identifier.
    pub account_id: String,
}

/// Selects zero or one account. Omitting it selects all accounts owned by the user.
#[derive(Debug, CommandArgs)]
pub struct OptionalAccountIdArgs {
    /// Optional stable account identifier.
    pub account_id: Option<String>,
}

/// Account lifecycle commands.
#[derive(Debug, BotCommand)]
#[command(
    name = "account",
    description = "Add, update, list, or remove China Unicom accounts"
)]
pub enum AccountCommand {
    /// Add an account and import its canonical four-field login JSON.
    Add(AccountAddArgs),
    /// Replace an account's credentials with a new four-field login JSON.
    Login(AccountIdArgs),
    /// List every China Unicom account owned by this bot user.
    List,
    /// Remove one account and its usage history.
    Remove(AccountIdArgs),
}

/// Scheduled-query task commands.
#[derive(Debug, BotCommand)]
#[command(
    name = "task",
    description = "Check or control scheduled China Unicom queries"
)]
pub enum TaskCommand {
    /// Start one account, or every account when account_id is omitted.
    Start(OptionalAccountIdArgs),
    /// Stop one account, or every account when account_id is omitted.
    Stop(OptionalAccountIdArgs),
    /// Show one account, or every account when account_id is omitted.
    Status(OptionalAccountIdArgs),
}

/// Per-account configuration commands.
#[derive(Debug, BotCommand)]
#[command(
    name = "config",
    description = "Show or change per-account configuration"
)]
pub enum ConfigCommand {
    /// Show one account, or every account when account_id is omitted.
    Show(OptionalAccountIdArgs),
    /// Interactively change one account.
    Set(AccountIdArgs),
}

/// The complete China Unicom command tree.
#[derive(Debug, BotCommand)]
#[command(
    name = "china_unicom",
    description = "Manage China Unicom accounts with canonical four-field login JSON"
)]
pub enum ChinaUnicomCommand {
    /// Add, update, list, or remove accounts.
    #[command(subcommand)]
    Account(AccountCommand),
    /// Query one account, or every account when account_id is omitted.
    Query(OptionalAccountIdArgs),
    /// Show or change per-account configuration.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Check or control scheduled query tasks.
    #[command(subcommand)]
    Task(TaskCommand),
}

#[cfg(test)]
mod tests {
    use oxidebot::commands::{CommandTree, FromCommandMatch};

    use super::{AccountCommand, ChinaUnicomCommand};

    #[test]
    fn parses_the_existing_multi_account_command_surface() {
        let command = ChinaUnicomCommand::command();
        let parsed = command
            .parse_message(&oxidebot::Message::text(
                "/china_unicom account add main --name 主卡",
            ))
            .expect("account add command parses");
        assert!(matches!(
            ChinaUnicomCommand::from_match(&parsed),
            Ok(ChinaUnicomCommand::Account(AccountCommand::Add(args)))
                if args.account_id == "main" && args.name.as_deref() == Some("主卡")
        ));

        let parsed = command
            .parse_message(&oxidebot::Message::text("/china_unicom query"))
            .expect("query command parses");
        assert!(matches!(
            ChinaUnicomCommand::from_match(&parsed),
            Ok(ChinaUnicomCommand::Query(args)) if args.account_id.is_none()
        ));
    }
}
