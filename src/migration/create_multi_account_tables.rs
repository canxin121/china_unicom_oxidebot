use sea_orm::ConnectionTrait;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Account::Table)
                    .if_not_exists()
                    .col(string(Account::Owner))
                    .col(string(Account::Id))
                    .col(string(Account::Bot))
                    .col(string(Account::Name))
                    .col(string(Account::TokenOnline))
                    .col(string(Account::AppId))
                    .col(text(Account::Cookie))
                    .col(string(Account::CapturedAt))
                    .col(string_null(Account::LastTokenRefreshAt))
                    .col(boolean(Account::EnableTask))
                    .col(integer(Account::Interval))
                    .col(integer_null(Account::Timeout))
                    .col(double_null(Account::FreeThreshold))
                    .col(double_null(Account::NonfreeThreshold))
                    .col(string(Account::QueryMode).default("auto"))
                    .col(double(Account::RefreshIntervalHours).default(12.0))
                    .primary_key(
                        Index::create()
                            .name("pk-unicom-account")
                            .col(Account::Owner)
                            .col(Account::Id),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(AccountState::Table)
                    .if_not_exists()
                    .col(string(AccountState::Owner))
                    .col(string(AccountState::AccountId))
                    .col(text_null(AccountState::PreviousSnapshot))
                    .col(text_null(AccountState::DailySnapshot))
                    .primary_key(
                        Index::create()
                            .name("pk-unicom-account-state")
                            .col(AccountState::Owner)
                            .col(AccountState::AccountId),
                    )
                    .to_owned(),
            )
            .await?;

        // Version 0.2 allowed one account per OxideBot user. Import it as `default`; the legacy
        // tables remain untouched so the migration is recoverable and idempotent.
        if manager.has_table("config").await? {
            manager
                .get_connection()
                .execute_unprepared(
                    r#"
                    INSERT OR IGNORE INTO unicom_account (
                        owner, account_id, bot, account_name, token_online, app_id, cookie,
                        captured_at, last_token_refresh_at, enable_task, interval, timeout,
                        free_threshold, nonfree_threshold, query_mode, refresh_interval_hours
                    )
                    SELECT
                        user, 'default', bot, '默认账号', token_online, app_id, cookie,
                        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), NULL, enable_task, interval,
                        timeout, free_threshold, nonfree_threshold, query_mode, 12.0
                    FROM config
                    "#,
                )
                .await?;
        }
        if manager.has_table("usage_state").await? {
            manager
                .get_connection()
                .execute_unprepared(
                    r#"
                    INSERT OR IGNORE INTO unicom_account_state (
                        owner, account_id, previous_snapshot, daily_snapshot
                    )
                    SELECT user, 'default', previous_snapshot, daily_snapshot
                    FROM usage_state
                    "#,
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AccountState::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Account::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Account {
    #[sea_orm(iden = "unicom_account")]
    Table,
    Owner,
    #[sea_orm(iden = "account_id")]
    Id,
    Bot,
    #[sea_orm(iden = "account_name")]
    Name,
    TokenOnline,
    AppId,
    Cookie,
    CapturedAt,
    LastTokenRefreshAt,
    EnableTask,
    Interval,
    Timeout,
    FreeThreshold,
    NonfreeThreshold,
    QueryMode,
    RefreshIntervalHours,
}

#[derive(DeriveIden)]
enum AccountState {
    #[sea_orm(iden = "unicom_account_state")]
    Table,
    Owner,
    AccountId,
    PreviousSnapshot,
    DailySnapshot,
}
