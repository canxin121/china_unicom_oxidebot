use std::fmt::Display;

use sea_orm::entity::prelude::*;

use china_unicom::utils::mask_secret;

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub user: String,
    pub bot: String,
    pub cookie: String,
    /// Legacy compatibility column. Authentication refresh is intentionally unsupported.
    pub token_online: String,
    /// Legacy compatibility column. Authentication refresh is intentionally unsupported.
    pub app_id: String,
    pub enable_task: bool,
    pub interval: i64,
    pub timeout: Option<i64>,
    pub free_threshold: Option<f64>,
    pub nonfree_threshold: Option<f64>,
    pub query_mode: String,
}

impl Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Cookie: {}", mask_secret(&self.cookie))?;
        writeln!(f, "Query mode: {}", self.query_mode)?;
        writeln!(
            f,
            "Task: {}",
            if self.enable_task {
                "running"
            } else {
                "stopped"
            }
        )?;
        writeln!(f, "Interval: {}s", self.interval)?;
        writeln!(
            f,
            "Timeout: {}",
            self.timeout
                .map(|value| format!("{value}s"))
                .unwrap_or_else(|| "None".into())
        )?;
        writeln!(
            f,
            "Free threshold: {}",
            self.free_threshold
                .map(|value| format!("{value:.3} GB"))
                .unwrap_or_else(|| "None".into())
        )?;
        write!(
            f,
            "Normal threshold: {}",
            self.nonfree_threshold
                .map(|value| format!("{value:.3} GB"))
                .unwrap_or_else(|| "None".into())
        )
    }
}

impl Default for Model {
    fn default() -> Self {
        Self {
            user: String::new(),
            bot: String::new(),
            cookie: String::new(),
            token_online: String::new(),
            app_id: String::new(),
            enable_task: true,
            interval: 300,
            timeout: Some(1800),
            free_threshold: None,
            nonfree_threshold: Some(0.05),
            query_mode: "auto".into(),
        }
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
