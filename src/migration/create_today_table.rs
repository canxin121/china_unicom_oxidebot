use crate::model::today::Column;
use sea_orm_migration::{prelude::*, schema::*};
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TodayTable::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Column::User)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(string(Column::Bot))
                    .col(string(Column::PackageName))
                    .col(float(Column::SumFlowUsed))
                    .col(float(Column::LimitFlowUsed))
                    .col(float(Column::NonLimitFlowUsed))
                    .col(float(Column::FreeFlowUsed))
                    .col(float(Column::NonFreeFlowUsed))
                    .col(float(Column::SumFlow))
                    .col(float(Column::LimitFlow))
                    .col(float(Column::NonLimitFlow))
                    .col(integer(Column::SumVoiceUsed))
                    .col(integer(Column::LimitVoiceUsed))
                    .col(integer(Column::NonLimitVoiceUsed))
                    .col(integer(Column::SumVoice))
                    .col(integer(Column::LimitVoice))
                    .col(integer(Column::NonLimitVoice))
                    .col(timestamp_with_time_zone(Column::Time))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TodayTable::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum TodayTable {
    #[sea_orm(iden = "today")]
    Table,
}
