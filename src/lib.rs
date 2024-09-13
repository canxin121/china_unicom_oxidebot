use std::{sync::Arc, time::Duration};

use anyhow::Result;
use clap::Parser;
use dashmap::DashMap;
use model::{ConfigActiveModel, ConfigEntity, ConfigModel, DailyEntity, LastEntity};
use oxidebot::{
    handler::Handler, manager::BroadcastSender, matcher::Matcher, source::message::MessageSegment,
    wait_user_text_generic, EasyBool, EventHandlerTrait,
};
use sea_orm::{EntityTrait, Set};
use tokio::task::JoinHandle;
use utils::{
    china_unicom::{create_china_unicom_task, query_once},
    db::init_db,
    option_t::OptionT,
    oxidebot_util::{get_user_bot_from, send_message},
};
pub mod cli;
pub mod migration;
pub mod model;
pub mod utils;

use crate::cli::Cli;

pub struct ChinaUnicomHandler {
    pub db: sea_orm::DatabaseConnection,
    pub tasks: Arc<DashMap<String, JoinHandle<()>>>,
    pub broadcast_sender: BroadcastSender,
}

impl ChinaUnicomHandler {
    pub async fn new(broadcast_sender: BroadcastSender) -> Handler {
        let self_ = Self {
            db: init_db().await.unwrap(),
            tasks: Arc::new(DashMap::new()),
            broadcast_sender,
        };
        self_.start_all_tasks().await.unwrap();
        Handler {
            event_handler: Some(Box::new(self_)),
            active_handler: None,
        }
    }
}

impl ChinaUnicomHandler {
    async fn start_all_tasks(&self) -> Result<()> {
        let configs = ConfigEntity::find().all(&self.db).await?;
        for config in configs {
            let tasks = self.tasks.clone();
            let db = self.db.clone();
            tokio::spawn(async move {
                match create_china_unicom_task(db, config.user.clone()).await {
                    Ok(task) => {
                        tasks.insert(config.user, task);
                    }
                    Err(e) => {
                        tracing::error!("ChinaUnicom: Task Auto start failed: {:?}", e);
                        let _ = send_message(
                            &config.user,
                            &config.bot,
                            format!("ChinaUnicom: Task Auto start failed: {:?}", e),
                        )
                        .await;
                    }
                }
            });
        }
        Ok(())
    }

    async fn send_message(&self, matcher: &Matcher, text: &str) -> Result<()> {
        matcher
            .try_send_message(vec![MessageSegment::text(text.to_string())])
            .await?;
        Ok(())
    }
    /// get user config, if not registered, send message to user
    async fn get_user_config(&self, matcher: &Matcher) -> Result<Option<ConfigModel>> {
        if let Some((user, _bot)) = get_user_bot_from(matcher).await {
            let config = ConfigEntity::find_by_id(user).one(&self.db).await?;
            if config.is_none() {
                let _ =  self.send_message(matcher, "You have not registered yet, please use the `register` command to register first.").await;
            }
            return Ok(config);
        }
        Ok(None)
    }

    async fn handle_register(&self, matcher: &Matcher, user: &str, bot: &str) -> Result<()> {
        let _ = matcher
            .try_send_message(vec![MessageSegment::text(
                "Please send your China Unicom Cookie in 30s.".to_string(),
            )])
            .await?;

        let (cookie, matcher) = wait_user_text_generic::<String>(
            matcher,
            &self.broadcast_sender,
            Duration::from_secs(30),
            1,
            None,
        )
        .await?;

        let _ = matcher
            .try_send_message(vec![MessageSegment::text(
                "Please send your China Unicom AppId in 30s.".to_string(),
            )])
            .await?;

        let (app_id, matcher) = wait_user_text_generic::<String>(
            &matcher,
            &self.broadcast_sender,
            Duration::from_secs(30),
            1,
            None,
        )
        .await?;

        let _ = matcher
            .try_send_message(vec![MessageSegment::text(
                "Please send your China Unicom TokenOnline in 30s.".to_string(),
            )])
            .await?;

        let (token_online, matcher) = wait_user_text_generic::<String>(
            &matcher,
            &self.broadcast_sender,
            Duration::from_secs(30),
            1,
            None,
        )
        .await?;

        let config = ConfigModel {
            user: user.to_string(),
            bot: bot.to_string(),
            app_id,
            token_online,
            cookie,
            ..Default::default()
        };

        let config_active: ConfigActiveModel = config.into();
        match ConfigEntity::insert(config_active).exec(&self.db).await {
            Ok(_) => {
                self.send_message(
                    &matcher,
                    "Register success, your task will be automatically started, you can use the `task` command to view the status of the task or control it.",
                )
                .await?;
                self.handle_add_task(&matcher, user).await?;
            }
            Err(sea_orm::DbErr::RecordNotInserted) => {
                self.send_message(
                    &matcher,
                    "You have already registered, if you want to update your cookie, please use the `set` command.",
                )
                .await?;
            }
            Err(e) => {
                self.send_message(
                    &matcher,
                    &format!("An error occurred while registering: {:?}", e),
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn handle_query(&self, matcher: &Matcher) -> Result<()> {
        if let Some(config) = self.get_user_config(matcher).await? {
            match query_once(&self.db, &config).await {
                Ok((_should_send, message)) => {
                    self.send_message(matcher, &message).await?;
                }
                Err(e) => {
                    self.send_message(
                        matcher,
                        &format!("An error occurred while querying: {:?}", e),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_deregister(&self, matcher: &Matcher, user: &str) -> Result<()> {
        let _ = matcher
            .try_send_message(vec![MessageSegment::text(
                "Are you sure you want to cancel the China Unicom Oxidebot service?\nThis will stop notification service and delete all your data.\nSend 'y' to confirm, 'n' to cancel."
                    .to_string(),
            )])
            .await?;
        let (easy_bool, matcher) = wait_user_text_generic::<EasyBool>(
            matcher,
            &self.broadcast_sender,
            Duration::from_secs(30),
            1,
            None,
        )
        .await?;

        // cancel
        if !easy_bool.0 {
            return Ok(());
        }

        if let Some((_user, task)) = self.tasks.remove(user) {
            task.abort();
        }

        let _ = LastEntity::delete_by_id(user).exec(&self.db).await;
        let _ = DailyEntity::delete_by_id(user).exec(&self.db).await;
        match ConfigEntity::delete_by_id(user).exec(&self.db).await {
            Ok(_) => {
                self.send_message(&matcher, "Deregister success.").await?;
            }
            Err(e) => match e {
                sea_orm::DbErr::RecordNotFound(_) => {
                    self.send_message(&matcher, "You have not registered yet.")
                        .await?;
                }
                other => {
                    self.send_message(
                        &matcher,
                        &format!(
                            "An error occurred while deregistering: {}",
                            other.to_string()
                        ),
                    )
                    .await?;
                }
            },
        }

        Ok(())
    }

    async fn handle_config_show(&self, matcher: &Matcher) -> Result<()> {
        if let Some(config) = self.get_user_config(matcher).await? {
            self.send_message(matcher, &config.to_string()).await?;
        }
        Ok(())
    }

    async fn handle_config_set(&self, matcher: &Matcher, user: &str) -> Result<()> {
        if let Some(config) = self.get_user_config(matcher).await? {
            matcher
                .try_send_message(vec![MessageSegment::text(
                    "Please send a option number to set:\n1.cookie: String\n2.interval: i64(seconds)\n3.timeout: i64(seconds) or None\n4.free_threshold: f64(GB) or None\n5.nonfree_threshold: f64(GB) or None\n\nSend 0 to cancel",
                )])
                .await?;

            let (option, matcher) = wait_user_text_generic::<u8>(
                matcher,
                &self.broadcast_sender,
                Duration::from_secs(30),
                3,
                Some("Please send a number between 0 and 5".to_string()),
            )
            .await?;

            #[allow(unused_assignments)]
            let mut config_active: Option<ConfigActiveModel> = None;
            match option {
                0 => {
                    matcher
                        .try_send_message(vec![MessageSegment::text(
                            "Config set operation cancel.",
                        )])
                        .await?;
                    return Ok(());
                }
                1 => {
                    let (cookie, _matcher) = wait_user_text_generic::<String>(
                        &matcher,
                        &self.broadcast_sender,
                        Duration::from_secs(30),
                        1,
                        None,
                    )
                    .await?;
                    let mut config_active1: ConfigActiveModel = config.into();
                    config_active1.cookie = Set(cookie);
                    config_active = Some(config_active1);
                }
                2 => {
                    let (interval, _matcher) = wait_user_text_generic::<i64>(
                        &matcher,
                        &self.broadcast_sender,
                        Duration::from_secs(30),
                        1,
                        Some("Please enter a valid number for interval.".to_string()),
                    )
                    .await?;
                    let mut config_active2: ConfigActiveModel = config.into();
                    config_active2.interval = Set(interval);
                    config_active = Some(config_active2);
                }
                3 => {
                    let (timeout, _matcher) = wait_user_text_generic::<OptionT<i64>>(
                        &matcher,
                        &self.broadcast_sender,
                        Duration::from_secs(30),
                        1,
                        Some("Please enter a valid number or 'none' for timeout.".to_string()),
                    )
                    .await?;
                    let mut config_active3: ConfigActiveModel = config.into();
                    config_active3.timeout = Set(timeout.0);
                    config_active = Some(config_active3);
                }
                4 => {
                    let (free_threshold, _matcher) = wait_user_text_generic::<OptionT<f64>>(
                        &matcher,
                        &self.broadcast_sender,
                        Duration::from_secs(30),
                        1,
                        Some(
                            "Please enter a valid number or 'none' for free_threshold.".to_string(),
                        ),
                    )
                    .await?;
                    let mut config_active4: ConfigActiveModel = config.into();
                    config_active4.free_threshold = Set(free_threshold.0);
                    config_active = Some(config_active4);
                }
                5 => {
                    let (nonfree_threshold, _matcher) = wait_user_text_generic::<OptionT<f64>>(
                        &matcher,
                        &self.broadcast_sender,
                        Duration::from_secs(30),
                        1,
                        Some(
                            "Please enter a valid number or 'none' for nonfree_threshold."
                                .to_string(),
                        ),
                    )
                    .await?;
                    let mut config_active5: ConfigActiveModel = config.into();
                    config_active5.nonfree_threshold = Set(nonfree_threshold.0);
                    config_active = Some(config_active5);
                }
                _ => {
                    matcher
                        .try_send_message(vec![MessageSegment::text(
                            "Invalid option number, exited.",
                        )])
                        .await?;
                    return Ok(());
                }
            }

            if let Some(config_active) = config_active {
                match ConfigEntity::update(config_active).exec(&self.db).await {
                    Ok(_) => {
                        let _ = self.send_message(&matcher, "Update success.").await;
                        self.handle_restart_task(&matcher, user).await?;
                    }
                    Err(e) => {
                        self.send_message(
                            &matcher,
                            &format!("An error occurred while updating: {:?}", e),
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn add_task(&self, user: &str) -> Result<()> {
        self.tasks.insert(
            user.to_string(),
            create_china_unicom_task(self.db.clone(), user.to_owned()).await?,
        );
        Ok(())
    }

    async fn handle_add_task(&self, matcher: &Matcher, user: &str) -> Result<()> {
        if let Some(config) = self.get_user_config(matcher).await? {
            if !config.enable_task {
                let mut config_active: ConfigActiveModel = config.into();
                config_active.enable_task = Set(true);
                match ConfigEntity::update(config_active).exec(&self.db).await {
                    Ok(_) => {}
                    Err(e) => {
                        self.send_message(
                            matcher,
                            &format!("ChinaUnicom: Task start failed to update config: {:?}", e),
                        )
                        .await?;
                        return Ok(());
                    }
                }
            }
            match self.add_task(user).await {
                Ok(_) => {
                    self.send_message(matcher, "ChinaUnicom: Task start success.")
                        .await?;
                }
                Err(e) => {
                    self.send_message(matcher, &format!("ChinaUnicom: Task start failed: {:?}", e))
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_task_stop(&self, matcher: &Matcher, user: &str) -> Result<()> {
        if let Some(config) = self.get_user_config(matcher).await? {
            if config.enable_task {
                let mut config_active: ConfigActiveModel = config.into();
                config_active.enable_task = Set(false);
                match ConfigEntity::update(config_active).exec(&self.db).await {
                    Ok(_) => {}
                    Err(e) => {
                        self.send_message(
                            matcher,
                            &format!("ChinaUnicom: Task stop failed to update config: {:?}", e),
                        )
                        .await?;
                        return Ok(());
                    }
                }
            }
            if let Some((_user, task)) = self.tasks.remove(user) {
                task.abort();
                self.send_message(matcher, "ChinaUnicom: Task stop success.")
                    .await?;
            } else {
                self.send_message(matcher, "ChinaUnicom: Task is not running.")
                    .await?;
            }
        }
        Ok(())
    }

    async fn handle_restart_task(&self, matcher: &Matcher, user: &str) -> Result<()> {
        if let Some((_user, task)) = self.tasks.remove(user) {
            task.abort();
            if let Some(config) = self.get_user_config(matcher).await? {
                if config.enable_task {
                    self.handle_add_task(matcher, user).await?;
                } else {
                    self.send_message(
                        matcher,
                        "ChinaUnicom: Task is stop, so it will not restart.",
                    )
                    .await?;
                }
            }
        } else {
            self.send_message(
                matcher,
                "ChinaUnicom: Task is not running, so it will not restart.",
            )
            .await?;
        }
        Ok(())
    }
    async fn handle_task_status(&self, matcher: &Matcher, user: &str) -> Result<()> {
        if self.tasks.contains_key(user) {
            self.send_message(matcher, "ChinaUnicom: Task is running.")
                .await?;
        } else {
            self.send_message(matcher, "ChinaUnicom: Task is not running.")
                .await?;
        }
        Ok(())
    }
}

impl EventHandlerTrait for ChinaUnicomHandler {
    #[must_use]
    #[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
    fn handle<'life0, 'async_trait>(
        &'life0 self,
        matcher: Matcher,
    ) -> ::core::pin::Pin<
        Box<dyn ::core::future::Future<Output = Result<()>> + ::core::marker::Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if let Some(message) = matcher.try_get_message() {
                let raw_text = message.get_raw_text();
                if !raw_text.starts_with(Cli::name()) {
                    return Ok(());
                }

                if matcher.is_group().await {
                    self.send_message(&matcher, "This command can only be used in private chat.")
                        .await?;
                    return Ok(());
                }

                let (user, bot) = get_user_bot_from(&matcher)
                    .await
                    .ok_or(anyhow::anyhow!("User ot bot not found"))?;

                match Cli::try_parse_from(
                    shlex::split(&raw_text).ok_or(anyhow::anyhow!("Parse shlex error"))?,
                ) {
                    Ok(cli) => match cli.command {
                        cli::Commands::Register => {
                            self.handle_register(&matcher, &user, &bot).await?;
                        }
                        cli::Commands::Query => {
                            self.handle_query(&matcher).await?;
                        }
                        cli::Commands::Task { task_command } => {
                            match task_command {
                                cli::TaskCommand::Start => {
                                    self.handle_add_task(&matcher, &user).await?;
                                }
                                cli::TaskCommand::Status => {
                                    self.handle_task_status(&matcher, &user).await?;
                                }
                                cli::TaskCommand::Stop => {
                                    self.handle_task_stop(&matcher, &user).await?;
                                }
                            };
                        }
                        cli::Commands::Config { config_command } => match config_command {
                            cli::ConfigCommand::Show => {
                                self.handle_config_show(&matcher).await?;
                            }
                            cli::ConfigCommand::Set => {
                                self.handle_config_set(&matcher, &user).await?;
                            }
                        },
                        cli::Commands::Deregister => {
                            self.handle_deregister(&matcher, &user).await?;
                        }
                    },
                    Err(e) => {
                        self.send_message(&matcher, &e.to_string()).await?;
                    }
                }
            }
            Ok(())
        })
    }
}
