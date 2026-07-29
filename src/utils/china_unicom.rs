use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use china_unicom::client::{AuthResult, ChinaUnicomClient};
use china_unicom::config::{AccountConfig, ParserConfig};
use china_unicom::models::UsageSnapshot;
use china_unicom::parser::UsageParser;
use china_unicom::utils::{legacy_credential_cookie, modern_credential_cookie, parse_iso_datetime};
use chrono::{Duration, Utc};
use dashmap::DashMap;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tokio::{sync::Mutex, task::JoinHandle, time::sleep};

use oxidebot::Message;
use oxidebot::delivery::BotDirectory;

use crate::model::{
    AccountActiveModel, AccountEntity, AccountModel, AccountStateActiveModel, AccountStateEntity,
    AccountStateModel,
};
use crate::report::{build_report, same_china_day};

use super::oxidebot_util::send_message;

static ACCOUNT_LOCKS: OnceLock<DashMap<String, Arc<Mutex<()>>>> = OnceLock::new();

fn account_lock(owner: &str, account_id: &str) -> Arc<Mutex<()>> {
    ACCOUNT_LOCKS
        .get_or_init(DashMap::new)
        .entry(format!("{owner}\0{account_id}"))
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn client_config(account: &AccountModel) -> AccountConfig {
    AccountConfig {
        id: account.account_id.clone(),
        name: account.account_name.clone(),
        app_id: account.app_id.clone(),
        app_id_env: String::new(),
        cookie: account.cookie.clone(),
        cookie_env: String::new(),
        token_online: account.token_online.clone(),
        token_online_env: String::new(),
        query_mode: account.query_mode.clone(),
        token_refresh_interval_hours: account.refresh_interval_hours,
    }
}

fn decode_snapshot(value: Option<&str>) -> Result<Option<UsageSnapshot>> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(serde_json::from_str)
        .transpose()
        .context("数据库中的流量快照已损坏")
}

async fn save_state(
    db: &sea_orm::DatabaseConnection,
    existing: Option<AccountStateModel>,
    account: &AccountModel,
    previous: &UsageSnapshot,
    daily: &UsageSnapshot,
) -> Result<()> {
    let previous_snapshot = serde_json::to_string(previous)?;
    let daily_snapshot = serde_json::to_string(daily)?;
    if let Some(existing) = existing {
        let mut active: AccountStateActiveModel = existing.into();
        active.previous_snapshot = Set(Some(previous_snapshot));
        active.daily_snapshot = Set(Some(daily_snapshot));
        active.update(db).await?;
    } else {
        AccountStateEntity::insert(AccountStateActiveModel {
            owner: Set(account.owner.clone()),
            account_id: Set(account.account_id.clone()),
            previous_snapshot: Set(Some(previous_snapshot)),
            daily_snapshot: Set(Some(daily_snapshot)),
        })
        .exec(db)
        .await?;
    }
    Ok(())
}

fn refresh_due(account: &AccountModel) -> bool {
    if account.token_online.trim().is_empty() || account.refresh_interval_hours <= 0.0 {
        return false;
    }
    let reference = account
        .last_token_refresh_at
        .as_deref()
        .unwrap_or(&account.captured_at);
    let Some(reference) = parse_iso_datetime(reference) else {
        return true;
    };
    Utc::now() - reference >= Duration::seconds((account.refresh_interval_hours * 3600.0) as i64)
}

async fn persist_refreshed_credentials(
    db: &sea_orm::DatabaseConnection,
    account: &mut AccountModel,
    refreshed: AuthResult,
) -> Result<()> {
    if !refreshed.token_online.is_empty() {
        account.token_online = refreshed.token_online;
    }
    account.cookie = refreshed.cookie;
    account.captured_at = refreshed.updated_at.clone();
    account.last_token_refresh_at = Some(refreshed.updated_at);
    let mut active: AccountActiveModel = account.clone().into();
    active.token_online = Set(account.token_online.clone());
    active.cookie = Set(account.cookie.clone());
    active.captured_at = Set(account.captured_at.clone());
    active.last_token_refresh_at = Set(account.last_token_refresh_at.clone());
    active.update(db).await?;
    Ok(())
}

async fn refresh_credentials(
    db: &sea_orm::DatabaseConnection,
    client: &ChinaUnicomClient,
    account: &mut AccountModel,
) -> Result<()> {
    anyhow::ensure!(
        !account.token_online.trim().is_empty(),
        "该账号没有可续期的 token_online，请重新发送四字段登录 JSON"
    );
    let refreshed = client
        .refresh_online(&account.token_online, &account.app_id, &account.cookie)
        .await?;
    persist_refreshed_credentials(db, account, refreshed).await
}

pub async fn query_once(
    db: &sea_orm::DatabaseConnection,
    account: &mut AccountModel,
) -> Result<(bool, Message)> {
    let lock = account_lock(&account.owner, &account.account_id);
    let _guard = lock.lock().await;
    *account = AccountEntity::find_by_id((account.owner.clone(), account.account_id.clone()))
        .one(db)
        .await?
        .context("联通账号在查询前已被删除")?;
    query_once_locked(db, account).await
}

async fn query_once_locked(
    db: &sea_orm::DatabaseConnection,
    account: &mut AccountModel,
) -> Result<(bool, Message)> {
    let client = ChinaUnicomClient::new(client_config(account), 20.0, true, true)?;
    let mut proactive_warning = None;
    if refresh_due(account)
        && let Err(error) = refresh_credentials(db, &client, account).await
    {
        tracing::warn!(
            owner = %account.owner,
            account = %account.account_id,
            %error,
            "联通凭据主动续期失败，继续尝试现有 Cookie"
        );
        proactive_warning = Some(format!("主动续期失败，已继续使用现有 Cookie：{error}"));
    }

    let query_result = match client
        .query(&account.cookie, Some(&account.query_mode))
        .await
    {
        Ok(result) => result,
        Err(error) if error.is_cookie_invalid() => {
            refresh_credentials(db, &client, account)
                .await
                .context("Cookie 已失效且自动续期失败，请重新发送四字段登录 JSON")?;
            client
                .query(&account.cookie, Some(&account.query_mode))
                .await?
        }
        Err(error) => return Err(error.into()),
    };
    let package_name = if query_result
        .payload
        .get("packageName")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .is_empty()
    {
        let selected_cookie = if query_result.query_mode.starts_with("modern") {
            modern_credential_cookie(&account.cookie)
        } else {
            legacy_credential_cookie(&account.cookie)
        };
        match client
            .query_package_name(if selected_cookie.is_empty() {
                &account.cookie
            } else {
                &selected_cookie
            })
            .await
        {
            Ok(name) => name,
            Err(error) => {
                tracing::warn!(
                    owner = %account.owner,
                    account = %account.account_id,
                    %error,
                    "套餐名称查询失败，不影响流量结果"
                );
                String::new()
            }
        }
    } else {
        String::new()
    };
    let mut current =
        UsageParser::new(&ParserConfig::default())?.parse(&query_result.payload, &package_name);
    if let Some(warning) = proactive_warning {
        current.warnings.push(warning);
    }
    let state = AccountStateEntity::find_by_id((account.owner.clone(), account.account_id.clone()))
        .one(db)
        .await?;
    let previous = decode_snapshot(
        state
            .as_ref()
            .and_then(|state| state.previous_snapshot.as_deref()),
    )?;
    let stored_daily = decode_snapshot(
        state
            .as_ref()
            .and_then(|state| state.daily_snapshot.as_deref()),
    )?;
    let daily = stored_daily
        .filter(|baseline| same_china_day(baseline, &current))
        .unwrap_or_else(|| current.clone());
    let report = build_report(
        account,
        &current,
        previous.as_ref(),
        Some(&daily),
        &query_result.query_mode,
    );
    let next_previous = match previous.as_ref() {
        Some(previous) if !report.should_notify => previous,
        _ => &current,
    };
    save_state(db, state, account, next_previous, &daily).await?;
    Ok((report.should_notify, report.message))
}

pub async fn create_china_unicom_task(
    db: sea_orm::DatabaseConnection,
    bots: BotDirectory,
    owner: String,
    account_id: String,
) -> Result<JoinHandle<()>> {
    let mut account = AccountEntity::find_by_id((owner.clone(), account_id.clone()))
        .one(&db)
        .await?
        .with_context(|| format!("联通账号 {account_id} 不存在"))?;
    anyhow::ensure!(account.enable_task, "定时任务未启用");
    anyhow::ensure!(account.interval >= 60, "查询间隔不能少于 60 秒");

    Ok(tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(account.interval as u64);
        let mut last_error = None::<String>;
        loop {
            match query_once(&db, &mut account).await {
                Ok((should_send, message)) => {
                    last_error = None;
                    if should_send
                        && let Err(error) = send_message(&bots, &owner, &account.bot, message).await
                    {
                        tracing::error!(%owner, account = %account.account_id, %error, "发送联通流量通知失败");
                    }
                }
                Err(error) => {
                    let error = error.to_string();
                    tracing::error!(%owner, account = %account.account_id, %error, "查询联通流量失败");
                    if last_error.as_deref() != Some(error.as_str()) {
                        let message = format!(
                            "联通账号 {} ({}) 查询失败：{error}\n如果登录包已失效，请使用 /china_unicom account login {} 重新发送四字段 JSON。",
                            account.account_name, account.account_id, account.account_id
                        );
                        if let Err(send_error) =
                            send_message(&bots, &owner, &account.bot, message).await
                        {
                            tracing::error!(%owner, account = %account.account_id, %send_error, "发送联通查询错误通知失败");
                        }
                        last_error = Some(error);
                    }
                }
            }
            sleep(interval).await;
        }
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{account_lock, client_config, refresh_due};
    use crate::model::AccountModel;

    fn account() -> AccountModel {
        AccountModel {
            owner: "telegram:1".into(),
            account_id: "main".into(),
            bot: "telegram:2".into(),
            account_name: "主卡".into(),
            token_online: "refresh".into(),
            app_id: "app".into(),
            cookie: "JUT=jwt; ecs_token=one; ecs_acc=two".into(),
            captured_at: "2020-01-01T00:00:00Z".into(),
            last_token_refresh_at: None,
            enable_task: true,
            interval: 300,
            timeout: Some(1800),
            free_threshold: None,
            nonfree_threshold: Some(0.05),
            query_mode: "auto".into(),
            refresh_interval_hours: 12.0,
        }
    }

    #[test]
    fn account_credentials_feed_upstream_client_and_refresh_schedule() {
        let mut account = account();
        let upstream = client_config(&account);
        assert_eq!(upstream.id, "main");
        assert_eq!(upstream.resolved_token_online(), "refresh");
        assert_eq!(upstream.resolved_app_id(), "app");
        assert_eq!(
            upstream.resolved_cookie(),
            "JUT=jwt; ecs_token=one; ecs_acc=two"
        );
        assert!(refresh_due(&account));

        account.token_online.clear();
        assert!(!refresh_due(&account));
        account.token_online = "refresh".into();
        account.refresh_interval_hours = 0.0;
        assert!(!refresh_due(&account));
        account.refresh_interval_hours = 12.0;
        account.last_token_refresh_at = Some("2999-01-01T00:00:00Z".into());
        assert!(!refresh_due(&account));
    }

    #[test]
    fn query_locks_are_shared_per_account_but_not_across_accounts() {
        let first = account_lock("owner", "main");
        let same = account_lock("owner", "main");
        let other = account_lock("owner", "backup");
        assert!(Arc::ptr_eq(&first, &same));
        assert!(!Arc::ptr_eq(&first, &other));
    }
}
