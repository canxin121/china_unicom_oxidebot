use sea_orm::EntityTrait;
use sea_orm_migration::prelude::*;

use crate::{
    model::{config::Column, ConfigEntity},
    utils::oxidebot_util::send_message,
};

use super::create_config_table::ConfigTable;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(ConfigTable::Table)
                    .add_column(
                        ColumnDef::new(Column::TokenOnline)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .add_column(
                        ColumnDef::new(Column::AppId)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .to_owned(),
            )
            .await;
        let db = manager.get_connection();
        let configs = ConfigEntity::find().all(db).await?;
        tokio::spawn(async move {
            for config_model in configs {
                if config_model.token_online == "" {
                    let _ =  send_message(
                        &config_model.user,
                        &config_model.bot,
                        "ChinaUnicom Update: please add 'token_online' and 'app_id' to you config, incase the cookie expire".to_string(),
                    ).await;
                }
            }
        });
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let _ = manager
            .alter_table(
                Table::alter()
                    .table(ConfigTable::Table)
                    .drop_column(Column::TokenOnline)
                    .drop_column(Column::AppId)
                    .to_owned(),
            )
            .await;
        Ok(())
    }
}
