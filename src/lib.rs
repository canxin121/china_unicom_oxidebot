//! China Unicom account, usage-query, and notification plugin for OxideBot 1.0.
//!
//! The plugin deliberately owns only its database connection and task registry.
//! Applications retain their own root state and expose this value with
//! `#[derive(oxidebot::BotState)]` when they have other services as well.

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use china_unicom::utils::{CredentialInput, parse_credential_input, parse_iso_datetime};
use dashmap::DashMap;
use model::{AccountActiveModel, AccountEntity, AccountModel, AccountStateEntity};
use oxidebot::{
    advanced::{Service, ServiceContext},
    delivery::BotDirectory,
    prelude::*,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use tokio::task::JoinHandle;
use utils::{
    china_unicom::{create_china_unicom_task, query_once},
    db::init_db,
    option_t::OptionT,
    oxidebot_util::participant_key,
};

use crate::cli::{
    AccountCommand, ChinaUnicomCommand, ConfigCommand, OptionalAccountIdArgs, TaskCommand,
};

pub mod cli;
pub mod migration;
pub mod model;
pub mod report;
pub mod utils;

const INPUT_TIMEOUT: Duration = Duration::from_secs(120);

/// Stable composite key for one owner's configured China Unicom account.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountKey {
    owner: String,
    account_id: String,
}

impl AccountKey {
    fn new(owner: impl Into<String>, account_id: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            account_id: account_id.into(),
        }
    }
}

/// Initialized China Unicom plugin state.
///
/// Construct it once with [`Self::open`], place a clone in the application
/// root state, then install [`Self::bundle`]. It preserves the existing SQLite
/// schema and the historical `platform_identifier` ownership keys, so a 0.1
/// deployment keeps its accounts and snapshots after migration.
#[derive(Clone)]
pub struct ChinaUnicomPlugin {
    db: sea_orm::DatabaseConnection,
    tasks: Arc<DashMap<AccountKey, JoinHandle<()>>>,
    bots: Arc<OnceLock<BotDirectory>>,
}

impl ChinaUnicomPlugin {
    /// Opens the private SQLite database and applies pending data migrations.
    pub async fn open() -> Result<Self> {
        Ok(Self {
            db: init_db().await?,
            tasks: Arc::new(DashMap::new()),
            bots: Arc::new(OnceLock::new()),
        })
    }

    /// Builds the reusable OxideBot 1.0 plugin bundle.
    ///
    /// The application root must expose `ChinaUnicomPlugin` through
    /// [`oxidebot::FromState`], which `#[derive(oxidebot::BotState)]` provides
    /// for a `#[state]` field. The bundle requires direct conversations and
    /// native plain-text delivery because it handles account credentials.
    pub fn bundle<S>(&self) -> PluginBundle<S>
    where
        S: Send + Sync + 'static,
        Self: FromState<S>,
    {
        PluginBundle::new("china-unicom")
            .version(env!("CARGO_PKG_VERSION"))
            .description("China Unicom multi-account usage queries and notifications")
            .require_capability("direct conversations", |capabilities| {
                capabilities.conversations.direct.is_supported()
            })
            .require_capability("outbound plain text", |capabilities| {
                capabilities.content.plain_text.is_supported()
            })
            .add(ChinaUnicomCommand::feature(china_unicom))
            .service(ChinaUnicomService {
                plugin: self.clone(),
            })
    }

    fn configured_bots(&self) -> Result<BotDirectory> {
        self.bots
            .get()
            .cloned()
            .context("China Unicom background service has not started")
    }

    async fn start_all_tasks(&self) -> Result<()> {
        for account in AccountEntity::find().all(&self.db).await? {
            if !account.enable_task {
                continue;
            }
            let key = AccountKey::new(account.owner.clone(), account.account_id.clone());
            match self.add_task(&account.owner, &account.account_id).await {
                Ok(()) => {}
                Err(error) => {
                    tracing::error!(
                        owner = %account.owner,
                        account = %account.account_id,
                        %error,
                        "自动启动联通查询任务失败"
                    );
                    self.tasks.remove(&key);
                }
            }
        }
        Ok(())
    }

    fn abort_all_tasks(&self) {
        for entry in self.tasks.iter() {
            entry.value().abort();
        }
        self.tasks.clear();
    }

    async fn send_message(&self, messenger: &Messenger, text: impl Into<String>) -> Result<()> {
        messenger.send(text.into()).await?;
        Ok(())
    }

    async fn prompt_text<T>(&self, dialogue: &Dialogue, prompt: &str, error: &str) -> Result<T>
    where
        T: std::str::FromStr + Send + 'static,
        T::Err: std::fmt::Display + Send + 'static,
    {
        Ok(dialogue
            .clone()
            .named("china-unicom-input")
            .timeout(INPUT_TIMEOUT)
            .question(prompt)
            .attempts(3)
            .error(error)
            .run()
            .await?)
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

    async fn prompt_login_package(&self, dialogue: &Dialogue) -> Result<CredentialInput> {
        let input = dialogue
            .clone()
            .named("china-unicom-login")
            .timeout(INPUT_TIMEOUT)
            .ask_text("请在 120 秒内直接发送 China Unicom Login 生成的四字段 JSON：\n{\"token_online\":\"...\",\"app_id\":\"...\",\"cookie\":\"...\",\"captured_at\":\"...\"}\n请勿拆分字段，也不要发送手机号、短信验证码或密码。")
            .await?;
        Self::parse_login_package(&input)
    }

    async fn handle_account_add(
        &self,
        messenger: &Messenger,
        dialogue: &Dialogue,
        owner: &str,
        bot: &str,
        account_id: &str,
        name: Option<String>,
    ) -> Result<()> {
        Self::validate_account_id(account_id)?;
        if self.account(owner, account_id).await?.is_some() {
            self.send_message(
                messenger,
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
        let credentials = self.prompt_login_package(dialogue).await?;
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
            messenger,
            format!("联通账号 `{account_id}` 已添加，定时查询与自动续期已启动。"),
        )
        .await
    }

    async fn handle_account_login(
        &self,
        messenger: &Messenger,
        dialogue: &Dialogue,
        owner: &str,
        account_id: &str,
    ) -> Result<()> {
        let Some(account) = self.account(owner, account_id).await? else {
            self.send_message(messenger, format!("联通账号 `{account_id}` 不存在。"))
                .await?;
            return Ok(());
        };
        let credentials = self.prompt_login_package(dialogue).await?;
        let mut active: AccountActiveModel = account.into();
        active.token_online = Set(credentials.token_online);
        active.app_id = Set(credentials.app_id);
        active.cookie = Set(credentials.cookie);
        active.captured_at = Set(credentials.captured_at);
        active.last_token_refresh_at = Set(None);
        active.update(&self.db).await?;
        self.restart_task(owner, account_id).await?;
        self.send_message(
            messenger,
            format!("联通账号 `{account_id}` 的四字段登录包已更新。"),
        )
        .await
    }

    async fn handle_account_list(&self, messenger: &Messenger, owner: &str) -> Result<()> {
        let accounts = self.accounts(owner).await?;
        if accounts.is_empty() {
            self.send_message(
                messenger,
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
        self.send_message(messenger, format!("联通账号：\n{lines}"))
            .await
    }

    async fn handle_account_remove(
        &self,
        messenger: &Messenger,
        dialogue: &Dialogue,
        owner: &str,
        account_id: &str,
    ) -> Result<()> {
        let Some(account) = self.account(owner, account_id).await? else {
            self.send_message(messenger, format!("联通账号 `{account_id}` 不存在。"))
                .await?;
            return Ok(());
        };
        let confirmed = dialogue
            .clone()
            .named("china-unicom-remove")
            .timeout(INPUT_TIMEOUT)
            .confirm(format!(
                "确定删除 {} ({}) 吗？登录凭据、快照和定时任务都会删除。",
                account.account_name, account.account_id
            ))
            .await?;
        if !confirmed {
            self.send_message(messenger, "已取消删除。").await?;
            return Ok(());
        }
        self.abort_task(owner, account_id);
        AccountStateEntity::delete_by_id((owner.to_owned(), account_id.to_owned()))
            .exec(&self.db)
            .await?;
        AccountEntity::delete_by_id((owner.to_owned(), account_id.to_owned()))
            .exec(&self.db)
            .await?;
        self.send_message(messenger, format!("联通账号 `{account_id}` 已删除。"))
            .await
    }

    async fn handle_query(
        &self,
        messenger: &Messenger,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(
                messenger,
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
                Ok((_, message)) => self.send_message(messenger, message).await?,
                Err(error) => {
                    self.send_message(
                        messenger,
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
        messenger: &Messenger,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(messenger, "没有匹配的联通账号。").await?;
            return Ok(());
        }
        self.send_message(
            messenger,
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
        messenger: &Messenger,
        dialogue: &Dialogue,
        owner: &str,
        account_id: &str,
    ) -> Result<()> {
        let Some(account) = self.account(owner, account_id).await? else {
            self.send_message(messenger, format!("联通账号 `{account_id}` 不存在。"))
                .await?;
            return Ok(());
        };
        let option = self
            .prompt_text::<u8>(
                dialogue,
                "请选择配置项：\n1. 四字段登录 JSON\n2. 账号显示名称\n3. 查询间隔（秒，至少 60）\n4. 最长通知间隔（秒，至少 60；None 关闭）\n5. 免流阈值（GB；None 关闭）\n6. 通用阈值（GB；None 关闭）\n7. 查询接口（auto/modern/legacy）\n8. 主动续期间隔（小时；0 关闭）\n0. 取消",
                "请输入 0 到 8。",
            )
            .await?;
        if option == 0 {
            self.send_message(messenger, "已取消修改。").await?;
            return Ok(());
        }
        if option == 1 {
            return self
                .handle_account_login(messenger, dialogue, owner, account_id)
                .await;
        }
        let mut active: AccountActiveModel = account.into();
        match option {
            2 => {
                let value = self
                    .prompt_text::<String>(dialogue, "请输入账号显示名称。", "名称不能为空。")
                    .await?;
                let value = value.trim();
                if value.is_empty() || value.chars().count() > 64 {
                    self.send_message(messenger, "名称必须为 1–64 个字符。")
                        .await?;
                    return Ok(());
                }
                active.account_name = Set(value.to_owned());
            }
            3 => {
                let value = self
                    .prompt_text::<i64>(
                        dialogue,
                        "请输入查询间隔秒数（至少 60）。",
                        "请输入有效整数。",
                    )
                    .await?;
                if value < 60 {
                    self.send_message(messenger, "查询间隔不能少于 60 秒。")
                        .await?;
                    return Ok(());
                }
                active.interval = Set(value);
            }
            4 => {
                let value = self
                    .prompt_text::<OptionT<i64>>(
                        dialogue,
                        "请输入最长通知间隔秒数（至少 60），或 None 关闭。",
                        "请输入有效整数或 None。",
                    )
                    .await?;
                if value.0.is_some_and(|value| value < 60) {
                    self.send_message(messenger, "最长通知间隔不能少于 60 秒。")
                        .await?;
                    return Ok(());
                }
                active.timeout = Set(value.0);
            }
            5 | 6 => {
                let value = self
                    .prompt_text::<OptionT<f64>>(
                        dialogue,
                        "请输入非负 GB 数值，或 None 关闭。",
                        "请输入有效数值或 None。",
                    )
                    .await?;
                if value
                    .0
                    .is_some_and(|value| !value.is_finite() || value < 0.0)
                {
                    self.send_message(messenger, "阈值必须是非负有限数值。")
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
                let value = self
                    .prompt_text::<String>(
                        dialogue,
                        "请输入 auto、modern 或 legacy。建议 auto。",
                        "请输入查询接口模式。",
                    )
                    .await?;
                let value = value.trim().to_ascii_lowercase();
                if !matches!(value.as_str(), "auto" | "modern" | "legacy") {
                    self.send_message(messenger, "查询接口只能是 auto、modern 或 legacy。")
                        .await?;
                    return Ok(());
                }
                active.query_mode = Set(value);
            }
            8 => {
                let value = self
                    .prompt_text::<f64>(
                        dialogue,
                        "请输入主动续期间隔小时数；0 关闭。",
                        "请输入非负数值。",
                    )
                    .await?;
                if !value.is_finite() || value < 0.0 {
                    self.send_message(messenger, "续期间隔必须是非负有限数值。")
                        .await?;
                    return Ok(());
                }
                active.refresh_interval_hours = Set(value);
            }
            _ => {
                self.send_message(messenger, "无效选项。").await?;
                return Ok(());
            }
        }
        active.update(&self.db).await?;
        self.restart_task(owner, account_id).await?;
        self.send_message(messenger, "配置已更新，相关定时任务已重启。")
            .await
    }

    async fn add_task(&self, owner: &str, account_id: &str) -> Result<()> {
        let key = AccountKey::new(owner, account_id);
        if self.tasks.contains_key(&key) {
            return Ok(());
        }
        let task = create_china_unicom_task(
            self.db.clone(),
            self.configured_bots()?,
            owner.to_owned(),
            account_id.to_owned(),
        )
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
        messenger: &Messenger,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(messenger, "没有匹配的联通账号。").await?;
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
        self.send_message(messenger, "所选联通账号的定时任务已启动。")
            .await
    }

    async fn handle_task_stop(
        &self,
        messenger: &Messenger,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(messenger, "没有匹配的联通账号。").await?;
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
        self.send_message(messenger, "所选联通账号的定时任务已停止。")
            .await
    }

    async fn handle_task_status(
        &self,
        messenger: &Messenger,
        owner: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        let accounts = self.selected_accounts(owner, account_id).await?;
        if accounts.is_empty() {
            self.send_message(messenger, "没有匹配的联通账号。").await?;
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
        self.send_message(messenger, status).await
    }

    async fn dispatch_command(
        &self,
        messenger: &Messenger,
        dialogue: &Dialogue,
        owner: &str,
        bot: &str,
        command: ChinaUnicomCommand,
    ) -> Result<()> {
        match command {
            ChinaUnicomCommand::Account(command) => match command {
                AccountCommand::Add(args) => {
                    self.handle_account_add(
                        messenger,
                        dialogue,
                        owner,
                        bot,
                        &args.account_id,
                        args.name,
                    )
                    .await
                }
                AccountCommand::Login(args) => {
                    self.handle_account_login(messenger, dialogue, owner, &args.account_id)
                        .await
                }
                AccountCommand::List => self.handle_account_list(messenger, owner).await,
                AccountCommand::Remove(args) => {
                    self.handle_account_remove(messenger, dialogue, owner, &args.account_id)
                        .await
                }
            },
            ChinaUnicomCommand::Query(OptionalAccountIdArgs { account_id }) => {
                self.handle_query(messenger, owner, account_id.as_deref())
                    .await
            }
            ChinaUnicomCommand::Config(command) => match command {
                ConfigCommand::Show(OptionalAccountIdArgs { account_id }) => {
                    self.handle_config_show(messenger, owner, account_id.as_deref())
                        .await
                }
                ConfigCommand::Set(args) => {
                    self.handle_config_set(messenger, dialogue, owner, &args.account_id)
                        .await
                }
            },
            ChinaUnicomCommand::Task(command) => match command {
                TaskCommand::Start(OptionalAccountIdArgs { account_id }) => {
                    self.handle_task_start(messenger, owner, account_id.as_deref())
                        .await
                }
                TaskCommand::Stop(OptionalAccountIdArgs { account_id }) => {
                    self.handle_task_stop(messenger, owner, account_id.as_deref())
                        .await
                }
                TaskCommand::Status(OptionalAccountIdArgs { account_id }) => {
                    self.handle_task_status(messenger, owner, account_id.as_deref())
                        .await
                }
            },
        }
    }
}

#[derive(Clone)]
struct ChinaUnicomService {
    plugin: ChinaUnicomPlugin,
}

#[async_trait]
impl<S> Service<S> for ChinaUnicomService
where
    S: Send + Sync + 'static,
{
    async fn run(
        &self,
        context: ServiceContext<S>,
    ) -> std::result::Result<(), oxidebot::runtime::ServiceError> {
        let _ = self.plugin.bots.set(context.bots().clone());
        self.plugin
            .start_all_tasks()
            .await
            .map_err(|error| oxidebot::runtime::ServiceError::new(error.to_string()))?;
        context.shutdown().cancelled().await;
        self.plugin.abort_all_tasks();
        Ok(())
    }
}

async fn china_unicom(
    Args(command): Args<ChinaUnicomCommand>,
    State(plugin): State<ChinaUnicomPlugin>,
    Sender(user): Sender,
    Conversation(conversation): Conversation,
    identity: BotIdentity,
    dialogue: Dialogue,
    messenger: Messenger,
) -> HandlerResult<()> {
    if conversation.kind != oxidebot::core::ConversationKind::Direct {
        messenger
            .send("该命令只能在私聊中使用，以免泄露登录凭据。")
            .await?;
        return Ok(());
    }
    let owner = participant_key(&identity, &user.id);
    let bot = participant_key(&identity, identity.bot.as_str());
    if let Err(error) = plugin
        .dispatch_command(&messenger, &dialogue, &owner, &bot, command)
        .await
    {
        tracing::error!(%owner, %error, "联通命令执行失败");
        messenger.send(format!("操作失败：{error}")).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ChinaUnicomPlugin;

    #[test]
    fn login_package_must_be_canonical_and_timestamped() {
        let valid = r#"{"token_online":"token","app_id":"app","cookie":"JUT=x","captured_at":"2026-07-27T03:00:07+08:00"}"#;
        let credentials = ChinaUnicomPlugin::parse_login_package(valid).expect("valid package");
        assert_eq!(credentials.app_id, "app");
        assert!(ChinaUnicomPlugin::parse_login_package("JUT=abc").is_err());
        assert!(ChinaUnicomPlugin::parse_login_package(
            r#"{"token_online":"token","app_id":"app","cookie":"JUT=x","captured_at":"not-a-date"}"#
        )
        .is_err());
    }

    #[test]
    fn account_ids_are_stable_and_command_safe() {
        assert!(ChinaUnicomPlugin::validate_account_id("main-1_backup").is_ok());
        assert!(ChinaUnicomPlugin::validate_account_id("").is_err());
        assert!(ChinaUnicomPlugin::validate_account_id("has spaces").is_err());
        assert!(ChinaUnicomPlugin::validate_account_id("中文").is_err());
    }
}
