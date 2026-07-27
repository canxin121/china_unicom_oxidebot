use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_column("config", "query_mode").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(ConfigTable::Table)
                        .add_column(
                            ColumnDef::new(ConfigTable::QueryMode)
                                .string()
                                .not_null()
                                .default("auto"),
                        )
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_table(
                Table::create()
                    .table(UsageState::Table)
                    .if_not_exists()
                    .col(string(UsageState::User).primary_key())
                    .col(text_null(UsageState::PreviousSnapshot))
                    .col(text_null(UsageState::DailySnapshot))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UsageState::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ConfigTable {
    #[sea_orm(iden = "config")]
    Table,
    QueryMode,
}

#[derive(DeriveIden)]
enum UsageState {
    Table,
    User,
    PreviousSnapshot,
    DailySnapshot,
}
