use china_unicom_rs::{data::ChinaUnicomData, query::query_china_unicom_data};
use chrono::TimeDelta;
use sea_orm::EntityTrait;
use tokio::{task::JoinHandle, time::sleep};

use crate::model::{
    today::build_today_data, ConfigEntity, ConfigModel, TodayActiveModel, TodayEntity, TodayModel,
    YesterdayActiveModel, YesterdayEntity, YesterdayModel,
};

use super::oxidebot_util::send_message;

// 一段时间内消耗的
const FORMAT_DURATION: &'static str = "[区间时长] 跳: [区间流量收费用量], 免: [区间流量免费用量]";

// 今日消耗的
const FORMAT_DAILY: &'static str = "今跳:[区间流量收费用量], 今免: [区间流量免费用量]";

// 余量
const FORMAT_LEFT: &'static str = "通用余: [流量通用余量], 定向余: [流量定向余量]";
// 用量
const FORMAT_USED: &'static str = "通用已用: [流量通用用量], 定向已用: [流量定向用量]";

pub async fn query_once(
    db: &sea_orm::DatabaseConnection,
    config: &ConfigModel,
) -> anyhow::Result<(bool, String)> {
    let new_data = query_china_unicom_data(&config.cookie).await?;
    let yesterday = YesterdayEntity::find_by_id(config.user.as_str())
        .one(db)
        .await?;
    let today = TodayEntity::find_by_id(config.user.as_str())
        .one(db)
        .await?;
    let mut should_send = false;
    let mut message = format!("{}:\n", new_data.package_name);

    match yesterday {
        Some(yesterday_model) => match today {
            Some(today_model) => {
                should_send = handle_update(&new_data, &today_model, config, db).await?;

                message += &new_data.format_with_last(&FORMAT_DURATION, &today_model.into())?;
                message += "\n";
                message += &new_data.format_with_last(&FORMAT_DAILY, &yesterday_model.into())?;
                message += "\n";
            }
            None => {
                // this should not happen
                YesterdayEntity::delete_by_id(config.user.as_str())
                    .exec(db)
                    .await?;

                let new_today_active: TodayActiveModel =
                    build_today_data(new_data.clone(), config.user.clone(), config.bot.clone());
                TodayEntity::update(new_today_active).exec(db).await?;
                message += &new_data.format(&FORMAT_DURATION)?;
                message += "\n";
                should_send = true;
            }
        },
        None => match today {
            Some(today_model) => {
                should_send = handle_update(&new_data, &today_model, config, db).await?;
                message += &new_data.format_with_last(&FORMAT_DURATION, &today_model.into())?;
                message += "\n";
            }
            None => {
                let new_today_active: TodayActiveModel =
                    build_today_data(new_data.clone(), config.user.clone(), config.bot.clone());
                TodayEntity::insert(new_today_active).exec(db).await?;
                should_send = true;
            }
        },
    }

    message += &new_data.format(&FORMAT_USED)?;
    message += "\n";
    message += &new_data.format(&FORMAT_LEFT)?;
    message += "\n";
    Ok((should_send, message))
}

async fn handle_update(
    new_data: &ChinaUnicomData,
    today_model: &TodayModel,
    config: &ConfigModel,
    db: &sea_orm::DatabaseConnection,
) -> anyhow::Result<bool> {
    let new_data_date = new_data.time.date_naive();
    let today_data_date = today_model.time.date_naive();

    if new_data_date != today_data_date {
        if today_data_date.succ_opt() == Some(new_data_date) {
            let update_data: YesterdayModel = today_model.clone().into();
            let update_active_data: YesterdayActiveModel = update_data.into();
            YesterdayEntity::update(update_active_data).exec(db).await?;
        } else {
            YesterdayEntity::delete_by_id(config.user.as_str())
                .exec(db)
                .await?;
        }
    }

    let mut should_update_today = false;

    if new_data_date != today_data_date {
        should_update_today = true
    } else if config.timeout.is_some()
        && new_data.time - today_model.time > TimeDelta::seconds(config.timeout.unwrap())
    {
        should_update_today = true
    } else if config.free_threshold.is_some()
        && new_data.free_flow_used - today_model.free_flow_used > config.free_threshold.unwrap()
    {
        should_update_today = true
    } else if config.nonfree_threshold.is_some()
        && new_data.non_free_flow_used - today_model.non_free_flow_used
            > config.nonfree_threshold.unwrap()
    {
        should_update_today = true
    }

    if should_update_today {
        let new_today_active: TodayActiveModel =
            build_today_data(new_data.clone(), config.user.clone(), config.bot.clone());
        tracing::info!("Update today data: {:?}", new_today_active);
        TodayEntity::update(new_today_active).exec(db).await?;
    }
    Ok(should_update_today)
}

pub async fn create_china_unicom_task<DB: Into<sea_orm::DatabaseConnection>>(
    db: DB,
    user: String,
) -> anyhow::Result<JoinHandle<()>> {
    let db = db.into();

    // 启动一个task，首先发送一个查询请求
    let config = ConfigEntity::find_by_id(&user)
        .one(&db)
        .await?
        .ok_or(anyhow::anyhow!("User {} not found in config", user))?;

    // 立刻查询一次
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
                                retry -= 1;
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
