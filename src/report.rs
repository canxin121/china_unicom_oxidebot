use std::collections::HashSet;

use china_unicom::models::UsageSnapshot;
use china_unicom::reporting::aggregate;
use china_unicom::utils::{format_data_mb, format_duration, parse_iso_datetime};

use crate::model::AccountModel;

#[derive(Debug, Clone)]
pub struct UsageReport {
    pub message: String,
    pub should_notify: bool,
}

pub fn same_china_day(first: &UsageSnapshot, second: &UsageSnapshot) -> bool {
    china_unicom::reporting::same_local_day(first, second, "Asia/Shanghai")
}

fn detail_lines(snapshot: &UsageSnapshot, limit: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for package in snapshot.packages.iter().take(limit) {
        let mut tags = vec![if package.free { "免流" } else { "通用" }.to_owned()];
        tags.push(
            if package.unlimited {
                "不限量"
            } else {
                "有限量"
            }
            .into(),
        );
        if package.has_carryover() {
            tags.push(format!(
                "结转剩{}",
                format_data_mb(package.carryover_remaining_mb)
            ));
        }
        let mut line = format!(
            "- {} [{}]：已用 {}，剩余 {}",
            package.name,
            tags.join(" / "),
            format_data_mb(package.used_mb),
            format_data_mb(package.remaining_mb)
        );
        if !package.end_date.is_empty() {
            line.push_str(&format!("，有效期 {}", package.end_date));
        }
        lines.push(line);
    }
    if snapshot.packages.len() > limit {
        lines.push(format!(
            "- 其余 {} 个流量包已省略",
            snapshot.packages.len() - limit
        ));
    }
    lines
}

pub fn build_report(
    account: &AccountModel,
    current: &UsageSnapshot,
    previous: Option<&UsageSnapshot>,
    today: Option<&UsageSnapshot>,
    query_mode: &str,
) -> UsageReport {
    let initial = previous.is_none();
    let previous = previous.unwrap_or(current);
    let today = today.unwrap_or(current);
    let categories = aggregate(current, previous, today);
    let normal = &categories["normal"];
    let normal_limited = &categories["normalLimited"];
    let normal_unlimited = &categories["normalUnlimited"];
    let free = &categories["free"];
    let free_limited = &categories["freeLimited"];
    let free_unlimited = &categories["freeUnlimited"];
    let elapsed_seconds = parse_iso_datetime(&current.captured_at)
        .zip(parse_iso_datetime(&previous.captured_at))
        .map(|(current, previous)| (current - previous).num_seconds().max(0))
        .unwrap_or(0);
    let previous_ids: HashSet<_> = previous
        .packages
        .iter()
        .map(|package| package.id.as_str())
        .collect();
    let new_packages: Vec<_> = if initial {
        Vec::new()
    } else {
        current
            .packages
            .iter()
            .filter(|package| !previous_ids.contains(package.id.as_str()))
            .collect()
    };

    let timeout_reached = account
        .timeout
        .is_some_and(|timeout| elapsed_seconds >= timeout);
    let free_reached = account
        .free_threshold
        .is_some_and(|threshold| free.interval_used_mb >= threshold.max(0.0) * 1024.0);
    let normal_reached = account
        .nonfree_threshold
        .is_some_and(|threshold| normal.interval_used_mb >= threshold.max(0.0) * 1024.0);
    let should_notify =
        !initial && (timeout_reached || free_reached || normal_reached || !new_packages.is_empty());

    let mut lines = vec![
        format!("{} ({})", account.account_name, account.account_id),
        current.package_name.clone(),
        format!(
            "区间 {}：通用 {}，免流 {}",
            format_duration(elapsed_seconds),
            format_data_mb(normal.interval_used_mb),
            format_data_mb(free.interval_used_mb)
        ),
        format!(
            "今日：通用 {}，免流 {}",
            format_data_mb(normal.today_used_mb),
            format_data_mb(free.today_used_mb)
        ),
        format!(
            "通用有限：已用 {}，剩余 {}",
            format_data_mb(normal_limited.used_mb),
            format_data_mb(normal_limited.remaining_mb)
        ),
        format!(
            "通用不限：已用 {}；免流有限剩余 {}；免流不限已用 {}",
            format_data_mb(normal_unlimited.used_mb),
            format_data_mb(free_limited.remaining_mb),
            format_data_mb(free_unlimited.used_mb)
        ),
    ];
    let carryover = normal.carryover_remaining_mb + free.carryover_remaining_mb;
    if carryover > 0.0 {
        lines.push(format!("结转剩余：{}", format_data_mb(carryover)));
    }
    if !new_packages.is_empty() {
        lines.push(format!(
            "新增流量包：{}",
            new_packages
                .iter()
                .map(|package| package.name.as_str())
                .collect::<Vec<_>>()
                .join("、")
        ));
    }
    lines.extend(
        current
            .warnings
            .iter()
            .map(|warning| format!("⚠ {warning}")),
    );
    lines.push(format!("查询接口：{query_mode}"));
    if !current.packages.is_empty() {
        lines.push("流量包明细：".into());
        lines.extend(detail_lines(current, 20));
    }
    UsageReport {
        message: lines.join("\n"),
        should_notify,
    }
}

#[cfg(test)]
mod tests {
    use china_unicom::models::FlowPackage;

    use super::*;

    fn account() -> AccountModel {
        AccountModel {
            owner: "telegram:1".into(),
            account_id: "main".into(),
            bot: "telegram:2".into(),
            account_name: "主卡".into(),
            token_online: "token".into(),
            app_id: "app".into(),
            cookie: "JUT=x".into(),
            captured_at: "2026-07-27T00:00:00Z".into(),
            last_token_refresh_at: None,
            enable_task: true,
            interval: 300,
            timeout: None,
            free_threshold: None,
            nonfree_threshold: Some(0.05),
            query_mode: "auto".into(),
            refresh_interval_hours: 12.0,
        }
    }

    fn snapshot(at: &str, used: f64) -> UsageSnapshot {
        UsageSnapshot {
            captured_at: at.into(),
            package_name: "测试套餐".into(),
            packages: vec![FlowPackage {
                id: "normal".into(),
                name: "通用".into(),
                total_mb: 1024.0,
                used_mb: used,
                remaining_mb: 1024.0 - used,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn threshold_is_account_scoped_and_counter_reset_is_safe() {
        let previous = snapshot("2026-07-27T00:00:00Z", 100.0);
        let current = snapshot("2026-07-27T00:10:00Z", 160.0);
        assert!(
            build_report(
                &account(),
                &current,
                Some(&previous),
                Some(&previous),
                "modern-get"
            )
            .should_notify
        );

        let reset = snapshot("2026-08-01T00:00:00Z", 5.0);
        assert_eq!(
            aggregate(&reset, &current, &reset)["normal"].interval_used_mb,
            0.0
        );
    }
}
