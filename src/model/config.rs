use std::fmt::Display;

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub user: String,
    pub bot: String,
    pub cookie: String,
    pub token_online: String,
    pub app_id: String,
    // 是否启用定时任务
    pub enable_task: bool,
    // 查询间隔(s, min = 60)
    pub interval: i64,
    // 超时时间(s, min = 60)
    pub timeout: Option<i64>,
    // 免费流量阈值(GB)
    pub free_threshold: Option<f64>,
    // 非免费流量阈值(GB)
    pub nonfree_threshold: Option<f64>,
}

impl Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 带有注释，方便查看
        writeln!(f, "Cookie: {}", self.cookie)?;
        writeln!(f, "Interval: {}s", self.interval)?;
        if let Some(timeout) = self.timeout {
            writeln!(f, "Timeout: {}s", timeout)?;
        } else {
            writeln!(f, "Timeout: None")?;
        }
        if let Some(free_threshold) = self.free_threshold {
            writeln!(f, "Free threshold: {:.2} GB", free_threshold)?;
        } else {
            writeln!(f, "Free threshold: None")?;
        }
        if let Some(nonfree_threshold) = self.nonfree_threshold {
            writeln!(f, "Nonfree threshold: {:.2} GB", nonfree_threshold)
        } else {
            writeln!(f, "Nonfree threshold: None")
        }
    }
}

impl Default for Model {
    fn default() -> Self {
        Self {
            user: String::with_capacity(0),
            enable_task: true,
            interval: 60,
            timeout: Some(1800),
            cookie: String::with_capacity(0),
            bot: String::with_capacity(0),
            free_threshold: None,
            nonfree_threshold: Some(0.05),
            token_online: String::with_capacity(0),
            app_id: String::with_capacity(0),
        }
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "super::last::Entity")]
    Today,
    #[sea_orm(has_one = "super::daily::Entity")]
    Yesterday,
}

impl Related<super::last::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Today.def()
    }
}

impl Related<super::daily::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Yesterday.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
