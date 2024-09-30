use anyhow::Result;
use china_unicom_rs::{data::ChinaUnicomData, online::online, query::query_china_unicom_data};
use chrono::TimeDelta;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tokio::{task::JoinHandle, time::sleep};

use crate::model::{
    daily::build_daily_active, last::build_last_active, ConfigActiveModel, ConfigEntity,
    ConfigModel, DailyActiveModel, DailyEntity, DailyModel, LastActiveModel, LastEntity, LastModel,
};

use super::oxidebot_util::send_message;

const FORMAT_LAST: &'static str = "[区间时长] 跳: [区间流量收费用量], 免: [区间流量免费用量]";

const FORMAT_DAILY: &'static str = "今跳:[区间流量收费用量], 今免: [区间流量免费用量]";

const FORMAT_LEFT: &'static str = "通用余: [流量通用余量], 定向余: [流量定向余量]";

const FORMAT_USED: &'static str = "通用已用: [流量通用用量], 定向已用: [流量定向用量]";

pub async fn query_once(
    db: &sea_orm::DatabaseConnection,
    config: &ConfigModel,
) -> anyhow::Result<(bool, String)> {
    // when the cookie is expired, we need to update the cookie
    let new_data = match query_china_unicom_data(&config.cookie).await {
        Ok(data) => data,
        Err(e) => {
            if e.to_string().contains("999998") {
                let new_config = handle_auth_update(config, db).await?;
                tracing::info!("Update auth info for user: {}", new_config.user);
                let data = query_china_unicom_data(&new_config.cookie).await?;
                data
            } else {
                return Err(e);
            }
        }
    };

    let daily_model = DailyEntity::find_by_id(config.user.as_str())
        .one(db)
        .await?;
    let last_model = LastEntity::find_by_id(config.user.as_str()).one(db).await?;

    let updated_last = handle_data_update(&new_data, &last_model, &daily_model, config, db).await?;

    let message = build_message(&new_data, last_model, daily_model)?;

    Ok((updated_last, message))
}

fn build_message(
    new_data: &ChinaUnicomData,
    last_model: Option<LastModel>,
    daily_model: Option<DailyModel>,
) -> Result<String> {
    let mut message = format!("{}:\n", new_data.package_name);
    match daily_model {
        Some(daily_model) => match last_model {
            Some(last_model) => {
                message += &new_data.format_with_last(&FORMAT_LAST, &last_model.into())?;
                message += "\n";
                message += &new_data.format_with_last(&FORMAT_DAILY, &daily_model.into())?;
                message += "\n";
            }
            None => {
                message += &new_data.format(&FORMAT_LAST)?;
                message += "\n";
            }
        },
        None => match last_model {
            Some(today_model) => {
                message += &new_data.format_with_last(&FORMAT_LAST, &today_model.into())?;
                message += "\n";
            }
            None => {}
        },
    }

    message += &new_data.format(&FORMAT_USED)?;
    message += "\n";
    message += &new_data.format(&FORMAT_LEFT)?;
    message += "\n";

    Ok(message)
}

fn should_update_last(
    config: &ConfigModel,
    new_data: &ChinaUnicomData,
    last_model: &Option<LastModel>,
) -> bool {
    if last_model.is_none() {
        return true;
    }
    let last_model = last_model.as_ref().unwrap();
    if let Some(timeout) = config.timeout {
        if new_data.time - last_model.time > TimeDelta::seconds(timeout) {
            return true;
        }
    }

    if let Some(free_threshold) = config.free_threshold {
        if new_data.free_flow_used - last_model.free_flow_used > free_threshold {
            return true;
        }
    }

    if let Some(nonfree_threshold) = config.nonfree_threshold {
        if new_data.non_free_flow_used - last_model.non_free_flow_used > nonfree_threshold {
            return true;
        }
    }

    false
}

async fn handle_data_update(
    new_data: &ChinaUnicomData,
    last_model: &Option<LastModel>,
    daily_model: &Option<DailyModel>,
    config: &ConfigModel,
    db: &sea_orm::DatabaseConnection,
) -> anyhow::Result<bool> {
    // handle daily data update
    // when the new_data time not equal to the daily data time or the daily data is not exist
    if daily_model.is_none()
        || new_data.time.date_naive() != daily_model.as_ref().unwrap().time.date_naive()
    {
        // delete the old daily data
        if daily_model.is_some() {
            DailyEntity::delete_by_id(config.user.as_str())
                .exec(db)
                .await?;
            tracing::info!("Delete old daily data for user: {}", config.user);
        }
        // insert the new daily data
        match last_model {
            Some(ref last_model) => {
                // if the last model is exist, we can use the last model to create the new daily model
                let new_daily_model: DailyModel = last_model.clone().into();

                let new_daily_active: DailyActiveModel = new_daily_model.into();
                DailyEntity::insert(new_daily_active).exec(db).await?;
                tracing::info!(
                    "Insert new daily data using last data for user: {}",
                    config.user
                );
            }
            None => {
                // if the last model is not exist, we need to create a new daily model

                let daily_data_active =
                    build_daily_active(new_data.clone(), config.user.clone(), config.bot.clone());
                DailyEntity::insert(daily_data_active).exec(db).await?;
                tracing::info!(
                    "Insert new daily data using new data for user: {}",
                    config.user
                );
            }
        }
    }

    // the judge of update last data is complex, so we need to extract it to a function
    let should_update_today = should_update_last(config, new_data, &last_model);

    if should_update_today {
        if last_model.is_some() {
            LastEntity::delete_by_id(config.user.as_str())
                .exec(db)
                .await?;
            tracing::info!("Delete old last data for user: {}", config.user);
        }

        let new_last_active: LastActiveModel =
            build_last_active(new_data.clone(), config.user.clone(), config.bot.clone());
        LastEntity::insert(new_last_active).exec(db).await?;
        tracing::info!("Insert new last data for user: {}", config.user);
    }
    Ok(should_update_today)
}

async fn handle_auth_update(
    config: &ConfigModel,
    db: &sea_orm::DatabaseConnection,
) -> Result<ConfigModel> {
    let resp = online(&config.token_online, &config.app_id).await?;
    let mut config_active: ConfigActiveModel = config.clone().into();
    config_active.token_online = Set(resp.online_token);
    config_active.cookie = Set(resp.cookie);
    let new_config = config_active.update(db).await?;
    Ok(new_config)
}

pub async fn create_china_unicom_task<DB: Into<sea_orm::DatabaseConnection>>(
    db: DB,
    user: String,
) -> anyhow::Result<JoinHandle<()>> {
    let db = db.into();

    let config = ConfigEntity::find_by_id(&user)
        .one(&db)
        .await?
        .ok_or(anyhow::anyhow!("User {} not found in config", user))?;

    let (shoudl_send, message) = query_once(&db, &config).await?;

    if shoudl_send {
        send_message(&user, &config.bot, message).await?;
    }

    let handle = tokio::spawn(async move {
        let mut retry = 3;
        let interval = std::time::Duration::from_secs(config.interval as u64);
        while retry > 0 {
            sleep(interval).await;
            match query_once(&db, &config).await {
                Ok((should_send, message)) => {
                    if should_send {
                        match send_message(&user, &config.bot, message).await {
                            Ok(_) => retry = 3,
                            Err(e) => {
                                tracing::error!(
                                    "[Retry: {}]Error when send message to user: {}",
                                    retry,
                                    e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "[Retry: {}]Error when query china unicom data: {}",
                        retry,
                        e
                    );
                    retry -= 1;
                }
            }
        }
    });
    Ok(handle)
}
