use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use china_unicom::utils::{CredentialInput, parse_credential_input, parse_iso_datetime};
use clap::Parser;
use dashmap::DashMap;
use model::{AccountActiveModel, AccountEntity, AccountModel, AccountStateEntity};
use oxidebot::{
    EasyBool, EventHandlerTrait, event::Event, handler::Handler, manager::BroadcastSender,
    matcher::Matcher, source::message::MessageSegment, wait_user_text_generic,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use tokio::task::JoinHandle;
use utils::{
    china_unicom::{create_china_unicom_task, query_once},
    db::init_db,
    option_t::OptionT,
    oxidebot_util::get_user_bot_from,
};

use crate::cli::{AccountCommand, Cli, Commands, ConfigCommand, TaskCommand};

pub mod cli;
pub mod migration;
pub mod model;
pub mod report;
pub mod utils;

const INPUT_TIMEOUT: Duration = Duration::from_secs(120);

/// The Telegram adapter emits a command interaction in addition to the
/// underlying message. Text commands must run only for that canonical message
/// event; otherwise one incoming command would be dispatched twice.
fn is_command_message_event(event: &Event) -> bool {
    matches!(event, Event::MessageEvent(_))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountKey {
    pub owner: String,
    pub account_id: String,
}

impl AccountKey {
    fn new(owner: impl Into<String>, account_id: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            account_id: account_id.into(),
        }
    }
}

pub struct ChinaUnicomHandler {
    pub db: sea_orm::DatabaseConnection,
    pub tasks: Arc<DashMap<AccountKey, JoinHandle<()>>>,
    pub broadcast_sender: BroadcastSender,
}

impl ChinaUnicomHandler {
    /// Backwards-compatible constructor. New integrations should use [`Self::try_new`].
    #[allow(clippy::new_ret_no_self)]
    pub async fn new(broadcast_sender: BroadcastSender) -> Handler {
        Self::try_new(broadcast_sender)
            .await
            .expect("failed to initialize China Unicom handler")
    }

    pub async fn try_new(broadcast_sender: BroadcastSender) -> Result<Handler> {
        let handler = Self {
            db: init_db().await?,
            tasks: Arc::new(DashMap::new()),
            broadcast_sender,
        };
        handler.start_all_tasks().await?;
        Ok(Handler {
            event_handler: Some(Box::new(handler)),
            active_handler: None,
        })
    }

    async fn start_all_tasks(&self) -> Result<()> {
        for account in AccountEntity::find().all(&self.db).await? {
            if !account.enable_task {
                continue;
            }
            let key = AccountKey::new(account.owner.clone(), account.account_id.clone());
            match create_china_unicom_task(
                self.db.clone(),
                account.owner.clone(),
                account.account_id.clone(),
            )
            .await
            {
                Ok(task) => {
                    self.tasks.insert(key, task);
                }
                Err(error) => {
                    tracing::error!(
                        owner = %account.owner,
                        account = %account.account_id,
                        %error,
                        "自动启动联通查询任务失败"
                    );
                }
            }
        }
        Ok(())
    }

    async fn send_message(&self, matcher: &Matcher, text: impl Into<String>) -> Result<()> {
        matcher
            .try_send_message(vec![MessageSegment::text(text.into())])
            .await?;
        Ok(())
    }

    async fn prompt_text<T>(
        &self,
        matcher: &Matcher,
        prompt: &str,
        error: &str,
    ) -> Result<(T, Matcher)>
    where
        T: std::str::FromStr + Send + 'static,
        T::Err: std::fmt::Debug,
    {
        self.send_message(matcher, prompt).await?;
        wait_user_text_generic::<T>(
            matcher,
            &self.broadcast_sender,
            INPUT_TIMEOUT,
            3,
            Some(error.to_owned()),
        )
        .await
    }

    async fn accounts(&self, owner: &str) -> Result<Vec<AccountModel>> {
        Ok(AccountEntity::find()
            .filter(model::account::Column::Owner.eq(owner))
            .order_by_asc(model::account::Column::AccountId)
            .all(&self.db)
            .await?)
    }

    async fn account(&self, owner: &str, account_id: &str) -> Result<Option<AccountModel>> {
        Ok(
            AccountEntity::find_by_id((owner.to_owned(), account_id.to_owned()))
                .one(&self.db)
                .await?,
        )
    }

    async fn selected_accounts(
        &self,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<Vec<AccountModel>> {
        if let Some(account_id) = account_id {
            return Ok(self.account(owner, account_id).await?.into_iter().collect());
        }
        self.accounts(owner).await
    }

    fn validate_account_id(account_id: &str) -> Result<()> {
        anyhow::ensure!(
            !account_id.is_empty()
                && account_id.len() <= 32
                && account_id
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric()
                        || matches!(character, '_' | '-')),
            "账号 ID 必须为 1–32 个 ASCII 字母、数字、下划线或连字符"
        );
        Ok(())
    }

    fn parse_login_package(input: &str) -> Result<CredentialInput> {
        anyhow::ensure!(
            input.trim_start().starts_with('{'),
            "请直接发送登录页面生成的四字段 JSON，不接受单独 Cookie"
        );
        let credentials = parse_credential_input(input)?;
        anyhow::ensure!(
            parse_iso_datetime(&credentials.captured_at).is_some(),
            "captured_at 必须是合法的 RFC 3339 时间"
        );
        Ok(credentials)
    }

    async fn prompt_login_package(&self, matcher: &Matcher) -> Result<(CredentialInput, Matcher)> {
        let (input, matcher) = self
            .prompt_text::<String>(
                matcher,
                "请在 120 秒内直接发送 China Unicom Login 生成的四字段 JSON：\n{\"token_online\":\"...\",\"app_id\":\"...\",\"cookie\":\"...\",\"captured_at\":\"...\"}\n请勿拆分字段，也不要发送手机号、短信验证码或密码。",
                "请输入完整的单行四字段 JSON。",
            )
            .await?;
        Ok((Self::parse_login_package(&input)?, matcher))
    }

    async fn handle_account_add(
        &self,
        matcher: &Matcher,
        owner: &str,
        bot: &str,
        account_id: &str,
        name: Option<String>,
    ) -> Result<()> {
        Self::validate_account_id(account_id)?;
        if self.account(owner, account_id).await?.is_some() {
            self.send_message(
                matcher,
                format!(
                    "联通账号 ID `{account_id}` 已存在；更新登录包请使用 /china_unicom account login {account_id}。"
                ),
            )
            .await?;
            return Ok(());
        }
        let account_name = name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| account_id.to_owned());
        anyhow::ensure!(
            account_name.chars().count() <= 64,
            "账号名称不能超过 64 个字符"
        );
        let (credentials, matcher) = self.prompt_login_package(matcher).await?;
        let account = AccountModel {
            owner: owner.to_owned(),
            account_id: account_id.to_owned(),
            bot: bot.to_owned(),
            account_name,
            token_online: credentials.token_online,
            app_id: credentials.app_id,
            cookie: credentials.cookie,
            captured_at: credentials.captured_at,
            last_token_refresh_at: None,
            enable_task: true,
            interval: 300,
            timeout: Some(1800),
            free_threshold: None,
            nonfree_threshold: Some(0.05),
            query_mode: "auto".into(),
            refresh_interval_hours: 12.0,
        };
        AccountEntity::insert(AccountActiveModel::from(account))
            .exec(&self.db)
            .await?;
        self.add_task(owner, account_id).await?;
        self.send_message(
            &matcher,
            format!("联通账号 `{account_id}` 已添加，定时查询与自动续期已启动。"),
        )
        .await
    }

    async fn handle_account_login(
        &self,
        matcher: &Matcher,
        owner: &str,
        account_id: &str,
    ) -> Result<()> {
        let Some(account) = self.account(owner, account_id).await? else {
            self.send_message(matcher, format!("联通账号 `{account_id}` 不存在。"))
                .await?;
            return Ok(());
        };
        let (credentials, matcher) = self.prompt_login_package(matcher).await?;
        let mut active: AccountActiveModel = account.into();
        active.token_online = Set(credentials.token_online);
        active.app_id = Set(credentials.app_id);
        active.cookie = Set(credentials.cookie);
        active.captured_at = Set(credentials.captured_at);
        active.last_token_refresh_at = Set(None);
        active.update(&self.db).await?;
        self.restart_task(owner, account_id).await?;
        self.send_message(
            &matcher,
            format!("联通账号 `{account_id}` 的四字段登录包已更新。"),
        )
        .await
    }

    async fn handle_account_list(&self, matcher: &Matcher, owner: &str) -> Result<()> {
        let accounts = self.accounts(owner).await?;
        if accounts.is_empty() {
            self.send_message(
                matcher,
                "还没有联通账号。使用 /china_unicom account add <账号ID> 添加。",
            )
            .await?;
            return Ok(());
        }
        let lines = accounts
            .iter()
            .map(|account| {
                let key = AccountKey::new(owner, &account.account_id);
                format!(
                    "- {} ({})：配置 {}，任务 {}",
                    account.account_name,
                    account.account_id,
                    if account.enable_task {
                        "启用"
                    } else {
                        "停用"
                    },
                    if self.tasks.contains_key(&key) {
                        "运行中"
                    } else {
                        "未运行"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.send_message(matcher, format!("联通账号：\n{lines}"))
            .await
    }

    async fn handle_account_remove(
        &self,
        matcher: &Matcher,
        owner: &str,
        account_id: &str,
    ) -> Result<()> {
        let Some(account) = self.account(owner, account_id).await? else {
            self.send_message(matcher, format!("联通账号 `{account_id}` 不存在。"))
                .await?;
            return Ok(());
        };
        let (confirmed, matcher) = self
            .prompt_text::<EasyBool>(
                matcher,
                &format!(
                    "确定删除 {} ({}) 吗？登录凭据、快照和定时任务都会删除。发送 y 确认，n 取消。",
                    account.account_name, account.account_id
                ),
                "请发送 y 或 n。",
            )
            .await?;
        if !confirmed.0 {
            self.send_message(&matcher, "已取消删除。").await?;
            return Ok(());
        }
        self.abort_task(owner, account_id);
        AccountStateEntity::delete_by_id((owner.to_owned(), account_id.to_owned()))
            .exec(&self.db)
            .await?;
        AccountEntity::delete_by_id((owner.to_owned(), account_id.to_owned()))
            .exec(&self.db)
            .await?;
        self.send_message(&matcher, format!("联通账号 `{account_id}` 已删除。"))
            .await
    }

    async fn handle_query(
        &self,
        matcher: &Matcher,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(
                matcher,
                account_id.map_or_else(
                    || "还没有联通账号。".to_owned(),
                    |account_id| format!("联通账号 `{account_id}` 不存在。"),
                ),
            )
            .await?;
            return Ok(());
        }
        for mut account in accounts {
            match query_once(&self.db, &mut account).await {
                Ok((_, message)) => self.send_message(matcher, message).await?,
                Err(error) => {
                    self.send_message(
                        matcher,
                        format!(
                            "联通账号 {} ({}) 查询失败：{error}\n登录包失效时请使用 /china_unicom account login {} 重新导入。",
                            account.account_name, account.account_id, account.account_id
                        ),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_config_show(
        &self,
        matcher: &Matcher,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(matcher, "没有匹配的联通账号。").await?;
            return Ok(());
        }
        self.send_message(
            matcher,
            accounts
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
        .await
    }

    async fn handle_config_set(
        &self,
        matcher: &Matcher,
        owner: &str,
        account_id: &str,
    ) -> Result<()> {
        let Some(account) = self.account(owner, account_id).await? else {
            self.send_message(matcher, format!("联通账号 `{account_id}` 不存在。"))
                .await?;
            return Ok(());
        };
        let (option, matcher) = self
            .prompt_text::<u8>(
                matcher,
                "请选择配置项：\n1. 四字段登录 JSON\n2. 账号显示名称\n3. 查询间隔（秒，至少 60）\n4. 最长通知间隔（秒，至少 60；None 关闭）\n5. 免流阈值（GB；None 关闭）\n6. 通用阈值（GB；None 关闭）\n7. 查询接口（auto/modern/legacy）\n8. 主动续期间隔（小时；0 关闭）\n0. 取消",
                "请输入 0 到 8。",
            )
            .await?;
        if option == 0 {
            self.send_message(&matcher, "已取消修改。").await?;
            return Ok(());
        }
        if option == 1 {
            return self.handle_account_login(&matcher, owner, account_id).await;
        }
        let mut active: AccountActiveModel = account.into();
        match option {
            2 => {
                let (value, _) = self
                    .prompt_text::<String>(&matcher, "请输入账号显示名称。", "名称不能为空。")
                    .await?;
                let value = value.trim();
                if value.is_empty() || value.chars().count() > 64 {
                    self.send_message(&matcher, "名称必须为 1–64 个字符。")
                        .await?;
                    return Ok(());
                }
                active.account_name = Set(value.to_owned());
            }
            3 => {
                let (value, _) = self
                    .prompt_text::<i64>(
                        &matcher,
                        "请输入查询间隔秒数（至少 60）。",
                        "请输入有效整数。",
                    )
                    .await?;
                if value < 60 {
                    self.send_message(&matcher, "查询间隔不能少于 60 秒。")
                        .await?;
                    return Ok(());
                }
                active.interval = Set(value);
            }
            4 => {
                let (value, _) = self
                    .prompt_text::<OptionT<i64>>(
                        &matcher,
                        "请输入最长通知间隔秒数（至少 60），或 None 关闭。",
                        "请输入有效整数或 None。",
                    )
                    .await?;
                if value.0.is_some_and(|value| value < 60) {
                    self.send_message(&matcher, "最长通知间隔不能少于 60 秒。")
                        .await?;
                    return Ok(());
                }
                active.timeout = Set(value.0);
            }
            5 | 6 => {
                let (value, _) = self
                    .prompt_text::<OptionT<f64>>(
                        &matcher,
                        "请输入非负 GB 数值，或 None 关闭。",
                        "请输入有效数值或 None。",
                    )
                    .await?;
                if value
                    .0
                    .is_some_and(|value| !value.is_finite() || value < 0.0)
                {
                    self.send_message(&matcher, "阈值必须是非负有限数值。")
                        .await?;
                    return Ok(());
                }
                if option == 5 {
                    active.free_threshold = Set(value.0);
                } else {
                    active.nonfree_threshold = Set(value.0);
                }
            }
            7 => {
                let (value, _) = self
                    .prompt_text::<String>(
                        &matcher,
                        "请输入 auto、modern 或 legacy。建议 auto。",
                        "请输入查询接口模式。",
                    )
                    .await?;
                let value = value.trim().to_ascii_lowercase();
                if !matches!(value.as_str(), "auto" | "modern" | "legacy") {
                    self.send_message(&matcher, "查询接口只能是 auto、modern 或 legacy。")
                        .await?;
                    return Ok(());
                }
                active.query_mode = Set(value);
            }
            8 => {
                let (value, _) = self
                    .prompt_text::<f64>(
                        &matcher,
                        "请输入主动续期间隔小时数；0 关闭。",
                        "请输入非负数值。",
                    )
                    .await?;
                if !value.is_finite() || value < 0.0 {
                    self.send_message(&matcher, "续期间隔必须是非负有限数值。")
                        .await?;
                    return Ok(());
                }
                active.refresh_interval_hours = Set(value);
            }
            _ => {
                self.send_message(&matcher, "无效选项。").await?;
                return Ok(());
            }
        }
        active.update(&self.db).await?;
        self.restart_task(owner, account_id).await?;
        self.send_message(&matcher, "配置已更新，相关定时任务已重启。")
            .await
    }

    async fn add_task(&self, owner: &str, account_id: &str) -> Result<()> {
        let key = AccountKey::new(owner, account_id);
        if self.tasks.contains_key(&key) {
            return Ok(());
        }
        let task =
            create_china_unicom_task(self.db.clone(), owner.to_owned(), account_id.to_owned())
                .await?;
        self.tasks.insert(key, task);
        Ok(())
    }

    fn abort_task(&self, owner: &str, account_id: &str) -> bool {
        self.tasks
            .remove(&AccountKey::new(owner, account_id))
            .map(|(_, task)| task.abort())
            .is_some()
    }

    async fn restart_task(&self, owner: &str, account_id: &str) -> Result<()> {
        self.abort_task(owner, account_id);
        let account = self
            .account(owner, account_id)
            .await?
            .context("配置在更新后消失")?;
        if account.enable_task {
            self.add_task(owner, account_id).await?;
        }
        Ok(())
    }

    async fn handle_task_start(
        &self,
        matcher: &Matcher,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(matcher, "没有匹配的联通账号。").await?;
            return Ok(());
        }
        for account in accounts {
            if !account.enable_task {
                let mut active: AccountActiveModel = account.clone().into();
                active.enable_task = Set(true);
                active.update(&self.db).await?;
            }
            self.add_task(owner, &account.account_id).await?;
        }
        self.send_message(matcher, "所选联通账号的定时任务已启动。")
            .await
    }

    async fn handle_task_stop(
        &self,
        matcher: &Matcher,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(matcher, "没有匹配的联通账号。").await?;
            return Ok(());
        }
        for account in accounts {
            if account.enable_task {
                let mut active: AccountActiveModel = account.clone().into();
                active.enable_task = Set(false);
                active.update(&self.db).await?;
            }
            self.abort_task(owner, &account.account_id);
        }
        self.send_message(matcher, "所选联通账号的定时任务已停止。")
            .await
    }

    async fn handle_task_status(
        &self,
        matcher: &Matcher,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(matcher, "没有匹配的联通账号。").await?;
            return Ok(());
        }
        let status = accounts
            .iter()
            .map(|account| {
                let running = self
                    .tasks
                    .contains_key(&AccountKey::new(owner, &account.account_id));
                format!(
                    "- {} ({})：配置 {}，运行状态 {}",
                    account.account_name,
                    account.account_id,
                    if account.enable_task {
                        "启用"
                    } else {
                        "停用"
                    },
                    if running { "运行中" } else { "未运行" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.send_message(matcher, status).await
    }

    async fn dispatch_command(
        &self,
        matcher: &Matcher,
        owner: &str,
        bot: &str,
        command: Commands,
    ) -> Result<()> {
        match command {
            Commands::Account { account_command } => match account_command {
                AccountCommand::Add { account_id, name } => {
                    self.handle_account_add(matcher, owner, bot, &account_id, name)
                        .await
                }
                AccountCommand::Login { account_id } => {
                    self.handle_account_login(matcher, owner, &account_id).await
                }
                AccountCommand::List => self.handle_account_list(matcher, owner).await,
                AccountCommand::Remove { account_id } => {
                    self.handle_account_remove(matcher, owner, &account_id)
                        .await
                }
            },
            Commands::Query { account_id } => {
                self.handle_query(matcher, owner, account_id.as_deref())
                    .await
            }
            Commands::Config { config_command } => match config_command {
                ConfigCommand::Show { account_id } => {
                    self.handle_config_show(matcher, owner, account_id.as_deref())
                        .await
                }
                ConfigCommand::Set { account_id } => {
                    self.handle_config_set(matcher, owner, &account_id).await
                }
            },
            Commands::Task { task_command } => match task_command {
                TaskCommand::Start { account_id } => {
                    self.handle_task_start(matcher, owner, account_id.as_deref())
                        .await
                }
                TaskCommand::Stop { account_id } => {
                    self.handle_task_stop(matcher, owner, account_id.as_deref())
                        .await
                }
                TaskCommand::Status { account_id } => {
                    self.handle_task_status(matcher, owner, account_id.as_deref())
                        .await
                }
            },
        }
    }
}

#[async_trait]
impl EventHandlerTrait for ChinaUnicomHandler {
    async fn handle(&self, matcher: Matcher) -> Result<()> {
        if !is_command_message_event(matcher.event.as_ref()) {
            return Ok(());
        }
        let Some(message) = matcher.try_get_message() else {
            return Ok(());
        };
        let raw_text = message.get_raw_text();
        if !raw_text.starts_with(Cli::name()) {
            return Ok(());
        }
        if matcher.is_group().await {
            self.send_message(&matcher, "该命令只能在私聊中使用，以免泄露登录凭据。")
                .await?;
            return Ok(());
        }
        let (owner, bot) = get_user_bot_from(&matcher)
            .await
            .context("无法识别用户或机器人")?;
        let args = shlex::split(&raw_text).context("命令引号不完整")?;
        match Cli::try_parse_from(args) {
            Ok(cli) => {
                if let Err(error) = self
                    .dispatch_command(&matcher, &owner, &bot, cli.command)
                    .await
                {
                    tracing::error!(%owner, %error, "联通命令执行失败");
                    self.send_message(&matcher, format!("操作失败：{error}"))
                        .await?;
                }
            }
            Err(error) => self.send_message(&matcher, error.to_string()).await?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use oxidebot::{
        event::{Event, MessageEvent},
        interaction::{InteractionEvent, InteractionKind},
        source::user::User,
    };

    use super::{ChinaUnicomHandler, is_command_message_event};

    fn command_interaction_event() -> Event {
        Event::InteractionEvent(InteractionEvent {
            id: "message-1:command".to_owned(),
            kind: InteractionKind::Command,
            action_id: Some("china_unicom".to_owned()),
            values: Vec::new(),
            user: User::default(),
            group: None,
            message: None,
            context_id: None,
            response: None,
            fields: BTreeMap::new(),
            command: None,
            locale: None,
            permissions: BTreeSet::new(),
            data: serde_json::Value::Null,
        })
    }

    #[test]
    fn command_handler_ignores_the_duplicate_command_interaction() {
        assert!(is_command_message_event(&Event::MessageEvent(
            MessageEvent::default()
        )));
        assert!(!is_command_message_event(&command_interaction_event()));
    }

    #[test]
    fn login_requires_exact_four_field_json() {
        let valid = r#"{"token_online":"refresh","app_id":"ChinaunicomMobileBusiness","cookie":"JUT=abc; ecs_token=one; ecs_acc=two","captured_at":"2026-07-27T12:00:00+08:00"}"#;
        let credentials = ChinaUnicomHandler::parse_login_package(valid).unwrap();
        assert_eq!(credentials.token_online, "refresh");
        assert_eq!(credentials.app_id, "ChinaunicomMobileBusiness");
        assert_eq!(credentials.cookie, "JUT=abc; ecs_token=one; ecs_acc=two");
        assert!(ChinaUnicomHandler::parse_login_package("JUT=abc").is_err());
        assert!(
            ChinaUnicomHandler::parse_login_package(
                r#"{"token_online":"refresh","cookie":"JUT=abc"}"#
            )
            .is_err()
        );
        assert!(
            ChinaUnicomHandler::parse_login_package(
                r#"{"token_online":"refresh","app_id":"app","cookie":"JUT=abc","captured_at":"not-a-time"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn account_ids_are_stable_and_command_safe() {
        assert!(ChinaUnicomHandler::validate_account_id("main-1_backup").is_ok());
        assert!(ChinaUnicomHandler::validate_account_id("").is_err());
        assert!(ChinaUnicomHandler::validate_account_id("has spaces").is_err());
        assert!(ChinaUnicomHandler::validate_account_id("中文").is_err());
    }
}
