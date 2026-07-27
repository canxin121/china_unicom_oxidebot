use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, Default, DeriveEntityModel)]
#[sea_orm(table_name = "unicom_account_state")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub owner: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub account_id: String,
    pub previous_snapshot: Option<String>,
    pub daily_snapshot: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
