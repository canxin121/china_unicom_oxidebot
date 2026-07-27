use chrono::{DateTime, Local};
use sea_orm::entity::prelude::*;

/// Legacy schema retained so existing installations can migrate without losing data.
#[derive(Clone, Debug, Default, DeriveEntityModel)]
#[sea_orm(table_name = "daily")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub user: String,
    pub bot: String,
    pub package_name: String,
    pub time: DateTime<Local>,
    pub sum_flow_used: f64,
    pub limit_flow_used: f64,
    pub non_limit_flow_used: f64,
    pub free_flow_used: f64,
    pub non_free_flow_used: f64,
    pub sum_flow: f64,
    pub limit_flow: f64,
    pub non_limit_flow: f64,
    pub sum_voice_used: i64,
    pub limit_voice_used: i64,
    pub non_limit_voice_used: i64,
    pub sum_voice: i64,
    pub limit_voice: i64,
    pub non_limit_voice: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
