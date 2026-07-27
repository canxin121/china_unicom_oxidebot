use std::fmt::Display;

use china_unicom::utils::mask_secret;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "unicom_account")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub owner: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub account_id: String,
    pub bot: String,
    pub account_name: String,
    pub token_online: String,
    pub app_id: String,
    pub cookie: String,
    pub captured_at: String,
    pub last_token_refresh_at: Option<String>,
    pub enable_task: bool,
    pub interval: i64,
    pub timeout: Option<i64>,
    pub free_threshold: Option<f64>,
    pub nonfree_threshold: Option<f64>,
    pub query_mode: String,
    pub refresh_interval_hours: f64,
}

impl Model {
    pub fn credentials_summary(&self) -> String {
        format!(
            "Cookie {}，token_online {}，app_id {}，采集于 {}",
            mask_secret(&self.cookie),
            mask_secret(&self.token_online),
            mask_secret(&self.app_id),
            self.captured_at
        )
    }
}

impl Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "账号: {} ({})", self.account_name, self.account_id)?;
        writeln!(f, "凭据: {}", self.credentials_summary())?;
        writeln!(f, "查询模式: {}", self.query_mode)?;
        writeln!(
            f,
            "定时任务: {}",
            if self.enable_task { "运行" } else { "停止" }
        )?;
        writeln!(f, "查询间隔: {}s", self.interval)?;
        writeln!(f, "主动续期间隔: {:.2}h", self.refresh_interval_hours)?;
        writeln!(
            f,
            "最长通知间隔: {}",
            self.timeout
                .map(|value| format!("{value}s"))
                .unwrap_or_else(|| "None".into())
        )?;
        writeln!(
            f,
            "免流阈值: {}",
            self.free_threshold
                .map(|value| format!("{value:.3} GB"))
                .unwrap_or_else(|| "None".into())
        )?;
        write!(
            f,
            "通用阈值: {}",
            self.nonfree_threshold
                .map(|value| format!("{value:.3} GB"))
                .unwrap_or_else(|| "None".into())
        )
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
