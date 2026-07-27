use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, Default, DeriveEntityModel)]
#[sea_orm(table_name = "usage_state")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub user: String,
    pub previous_snapshot: Option<String>,
    pub daily_snapshot: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
