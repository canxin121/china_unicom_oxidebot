use sea_orm_migration::{prelude::*, schema::*};

use crate::model::config::Column;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ConfigTable::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Column::User)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(string(Column::Bot))
                    .col(string(Column::Cookie))
                    .col(boolean(Column::EnableTask))
                    .col(integer(Column::Interval))
                    .col(ColumnDef::new(Column::Timeout).integer().null())
                    .col(ColumnDef::new(Column::FreeThreshold).float().null())
                    .col(ColumnDef::new(Column::NonfreeThreshold).float().null())
                    .col(ColumnDef::new(Column::TokenOnline).string().not_null())
                    .col(ColumnDef::new(Column::AppId).string().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ConfigTable::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum ConfigTable {
    #[sea_orm(iden = "config")]
    Table,
}
