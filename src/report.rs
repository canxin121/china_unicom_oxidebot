use china_unicom::{
    models::{FlowPackage, UsageSnapshot},
    reporting::{same_local_day, summarize_usage},
    utils::format_data_mb,
};
use oxidebot::{
    Message,
    message::{RichText, TextSpan, TextStyle},
};

use crate::model::AccountModel;

#[derive(Debug, Clone)]
pub struct UsageReport {
    /// Full on-demand reply, including the current balance and package detail.
    pub reply: Message,
    /// Compact background notification whose first two lines remain useful in
    /// Telegram's Android notification preview.
    pub notification: Message,
    pub should_notify: bool,
}

#[derive(Default)]
struct RichReport {
    text: String,
    spans: Vec<TextSpan>,
}

impl RichReport {
    fn text(&mut self, value: impl AsRef<str>) {
        self.text.push_str(value.as_ref());
    }

    fn bold(&mut self, value: impl AsRef<str>) {
        self.styled(value, TextStyle::Bold);
    }

    fn code(&mut self, value: impl AsRef<str>) {
        self.styled(value, TextStyle::Code);
    }

    fn styled(&mut self, value: impl AsRef<str>, style: TextStyle) {
        let value = value.as_ref();
        let start = self.text.len();
        self.text.push_str(value);
        self.spans.push(TextSpan {
            range: start..self.text.len(),
            styles: vec![style],
        });
    }

    fn line(&mut self) {
        self.text.push('\n');
    }

    fn into_message(self) -> Message {
        Message::rich_text(RichText {
            text: self.text,
            spans: self.spans,
        })
    }
}

pub fn same_china_day(first: &UsageSnapshot, second: &UsageSnapshot) -> bool {
    same_local_day(first, second, "Asia/Shanghai")
}

fn usage_line(report: &mut RichReport, label: &str, used: f64, remaining: f64) {
    report.bold(label);
    report.text("  已用 ");
    report.code(format_data_mb(used));
    report.text("  ·  剩余 ");
    report.code(format_data_mb(remaining));
    report.line();
}

fn package_line(report: &mut RichReport, package: &FlowPackage) {
    report.text("• ");
    report.bold(&package.name);
    report.text("  已用 ");
    report.code(format_data_mb(package.used_mb));
    report.text("  ·  剩余 ");
    report.code(format_data_mb(package.remaining_mb));
    if !package.end_date.is_empty() {
        report.text("  ·  至 ");
        report.text(&package.end_date);
    }
    report.line();
}

fn elapsed_label(seconds: i64) -> String {
    let mut remaining = seconds.max(0);
    let days = remaining / 86_400;
    remaining %= 86_400;
    let hours = remaining / 3_600;
    remaining %= 3_600;
    let minutes = remaining / 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days} 天"));
    }
    if hours > 0 {
        parts.push(format!("{hours} 小时"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes} 分钟"));
    }
    if parts.is_empty() {
        "不足 1 分钟".into()
    } else {
        parts.join(" ")
    }
}

fn interval_label(initial: bool, elapsed_seconds: i64) -> Option<String> {
    (!initial).then(|| {
        if elapsed_seconds > 0 {
            format!("近 {}", elapsed_label(elapsed_seconds))
        } else {
            "较上次查询".into()
        }
    })
}

fn consumption_line(report: &mut RichReport, label: &str, normal: f64, free: f64) {
    if !label.is_empty() {
        report.text(label);
        report.text("  ");
    }
    report.text("通用 ");
    report.code(format_data_mb(normal));
    report.text("  ·  免流 ");
    report.code(format_data_mb(free));
    report.line();
}

fn balance_line(report: &mut RichReport, normal_remaining: f64, free_remaining: f64) {
    report.text("余量  通用 ");
    report.code(format_data_mb(normal_remaining));
    report.text("  ·  免流 ");
    report.code(format_data_mb(free_remaining));
    report.line();
}

fn build_notification(
    account: &AccountModel,
    interval: Option<&str>,
    normal_interval_used_mb: f64,
    free_interval_used_mb: f64,
    normal_remaining_mb: f64,
    free_remaining_mb: f64,
    new_package_count: usize,
) -> Message {
    let period = interval.unwrap_or("刚刚");
    let has_usage = normal_interval_used_mb > 0.0 || free_interval_used_mb > 0.0;
    let mut report = RichReport::default();

    if has_usage {
        report.text("📉 ");
        report.bold(&account.account_name);
        report.text("  ·  ");
        report.text(period);
        report.text("消耗");
        report.line();
        consumption_line(
            &mut report,
            "",
            normal_interval_used_mb,
            free_interval_used_mb,
        );
    } else if new_package_count > 0 {
        report.text("🆕 ");
        report.bold(&account.account_name);
        report.text("  ·  ");
        report.text(period);
        report.line();
        report.text(format!("新增 {new_package_count} 个流量包"));
        report.line();
    } else {
        report.text("⏱ ");
        report.bold(&account.account_name);
        report.text("  ·  ");
        report.text(period);
        report.text(" 无流量变化");
        report.line();
    }

    if has_usage && new_package_count > 0 {
        report.text(format!("新增 {new_package_count} 个流量包"));
        report.line();
    }
    balance_line(&mut report, normal_remaining_mb, free_remaining_mb);
    report.into_message()
}

pub fn build_report(
    account: &AccountModel,
    current: &UsageSnapshot,
    previous: Option<&UsageSnapshot>,
    today: Option<&UsageSnapshot>,
    _query_mode: &str,
) -> UsageReport {
    let initial = previous.is_none();
    let previous = previous.unwrap_or(current);
    let today = today.unwrap_or(current);
    let summary = summarize_usage(current, previous, today);
    let normal = &summary.categories["normal"];
    let free = &summary.categories["free"];
    let normal_unlimited = &summary.categories["normalUnlimited"];
    let free_unlimited = &summary.categories["freeUnlimited"];
    let timeout_reached = account
        .timeout
        .is_some_and(|timeout| summary.elapsed_seconds >= timeout);
    let free_reached = account
        .free_threshold
        .is_some_and(|threshold| free.interval_used_mb >= threshold.max(0.0) * 1024.0);
    let normal_reached = account
        .nonfree_threshold
        .is_some_and(|threshold| normal.interval_used_mb >= threshold.max(0.0) * 1024.0);
    let should_notify = !initial
        && (timeout_reached || free_reached || normal_reached || !summary.new_packages.is_empty());

    let interval = interval_label(initial, summary.elapsed_seconds);
    let notification = build_notification(
        account,
        interval.as_deref(),
        normal.interval_used_mb,
        free.interval_used_mb,
        normal.remaining_mb,
        free.remaining_mb,
        summary.new_packages.len(),
    );

    let mut report = RichReport::default();
    report.text("📊 ");
    report.bold(&account.account_name);
    report.text("  ·  ");
    report.code(&account.account_id);
    report.line();
    if !current.package_name.trim().is_empty() {
        report.text("套餐  ");
        report.text(&current.package_name);
        report.line();
    }
    report.line();
    report.bold("余量");
    report.line();
    usage_line(&mut report, "通用", normal.used_mb, normal.remaining_mb);
    usage_line(&mut report, "免流", free.used_mb, free.remaining_mb);

    report.line();
    if let Some(interval) = interval.as_deref() {
        report.bold("变化");
        report.line();
        let interval_consumption = format!("{interval}消耗");
        consumption_line(
            &mut report,
            &interval_consumption,
            normal.interval_used_mb,
            free.interval_used_mb,
        );
        consumption_line(
            &mut report,
            "今日累计",
            normal.today_used_mb,
            free.today_used_mb,
        );
    }

    let carryover = normal.carryover_remaining_mb + free.carryover_remaining_mb;
    if carryover > 0.0 || normal_unlimited.used_mb > 0.0 || free_unlimited.used_mb > 0.0 {
        report.line();
        report.bold("其他");
        report.line();
        if carryover > 0.0 {
            report.text("结转剩余  ");
            report.code(format_data_mb(carryover));
            report.line();
        }
        if normal_unlimited.used_mb > 0.0 || free_unlimited.used_mb > 0.0 {
            report.text("不限量已用  通用 ");
            report.code(format_data_mb(normal_unlimited.used_mb));
            report.text("  ·  免流 ");
            report.code(format_data_mb(free_unlimited.used_mb));
            report.line();
        }
    }

    if !summary.new_packages.is_empty() {
        report.line();
        report.bold("新增流量包");
        report.line();
        for package in summary.new_packages.iter().take(3) {
            report.text("• ");
            report.text(&package.name);
            report.line();
        }
        if summary.new_packages.len() > 3 {
            report.text(format!(
                "另有 {} 个新增流量包",
                summary.new_packages.len() - 3
            ));
            report.line();
        }
    }

    let package_details = current
        .packages
        .iter()
        .filter(|package| package.total_mb > 0.0 || package.unlimited)
        .take(3)
        .collect::<Vec<_>>();
    if !package_details.is_empty() {
        report.line();
        report.bold("套餐明细");
        report.line();
        for package in package_details {
            package_line(&mut report, package);
        }
        if current.packages.len() > 3 {
            report.text(format!(
                "其余 {} 个流量包已省略",
                current.packages.len() - 3
            ));
            report.line();
        }
    }

    for warning in &current.warnings {
        report.line();
        report.text("⚠ ");
        report.text(warning);
        report.line();
    }
    UsageReport {
        reply: report.into_message(),
        notification,
        should_notify,
    }
}

#[cfg(test)]
mod tests {
    use china_unicom::models::FlowPackage;

    use super::*;

    fn account() -> AccountModel {
        AccountModel {
            owner: "telegram_1".into(),
            account_id: "main".into(),
            bot: "telegram_2".into(),
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
                name: "通用流量".into(),
                total_mb: 1024.0,
                used_mb: used,
                remaining_mb: 1024.0 - used,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn replies_show_the_real_interval_and_notifications_lead_with_usage() {
        let previous = snapshot("2026-07-27T00:00:00Z", 100.0);
        let current = snapshot("2026-07-27T00:10:00Z", 160.0);
        let report = build_report(
            &account(),
            &current,
            Some(&previous),
            Some(&previous),
            "modern-get",
        );
        assert!(report.should_notify);
        let Message { segments, .. } = report.reply;
        let [oxidebot::core::source::message::MessageSegment::RichText(value)] =
            segments.as_slice()
        else {
            panic!("report must retain portable rich-text formatting");
        };
        assert!(value.text.contains("余量"));
        assert!(value.text.contains("近 10 分钟"));
        assert!(value.text.contains("近 10 分钟消耗"));
        assert!(value.text.contains("通用 60MB"));
        assert!(!value.text.contains("本次"));
        assert!(value.text.contains("套餐明细"));
        assert!(!value.spans.is_empty());
        assert!(value.text.lines().count() < 20);

        let Message { segments, .. } = report.notification;
        let [oxidebot::core::source::message::MessageSegment::RichText(value)] =
            segments.as_slice()
        else {
            panic!("notification must retain portable rich-text formatting");
        };
        let lines = value.text.lines().collect::<Vec<_>>();
        assert_eq!(lines[0], "📉 主卡  ·  近 10 分钟消耗");
        assert_eq!(lines[1], "通用 60MB  ·  免流 0MB");
        assert_eq!(lines[2], "余量  通用 864MB  ·  免流 0MB");
    }

    #[test]
    fn initial_query_has_no_zero_second_change_line() {
        let current = snapshot("2026-07-27T00:00:00Z", 160.0);
        let report = build_report(&account(), &current, None, Some(&current), "modern-get");
        let Message { segments, .. } = report.reply;
        let [oxidebot::core::source::message::MessageSegment::RichText(value)] =
            segments.as_slice()
        else {
            panic!("report must retain portable rich-text formatting");
        };
        assert!(!value.text.contains("近 0"));
        assert!(!value.text.contains("变化"));
    }

    #[test]
    fn timeout_notification_explains_that_the_interval_had_no_usage() {
        let mut account = account();
        account.timeout = Some(300);
        let previous = snapshot("2026-07-27T00:00:00Z", 100.0);
        let current = snapshot("2026-07-27T00:05:00Z", 100.0);
        let report = build_report(
            &account,
            &current,
            Some(&previous),
            Some(&previous),
            "modern-get",
        );
        assert!(report.should_notify);

        let Message { segments, .. } = report.notification;
        let [oxidebot::core::source::message::MessageSegment::RichText(value)] =
            segments.as_slice()
        else {
            panic!("notification must retain portable rich-text formatting");
        };
        let lines = value.text.lines().collect::<Vec<_>>();
        assert_eq!(lines[0], "⏱ 主卡  ·  近 5 分钟 无流量变化");
        assert_eq!(lines[1], "余量  通用 924MB  ·  免流 0MB");
    }

    #[test]
    fn new_package_notification_leads_with_the_package_event() {
        let previous = snapshot("2026-07-27T00:00:00Z", 100.0);
        let mut current = snapshot("2026-07-27T00:05:00Z", 100.0);
        current.packages.push(FlowPackage {
            id: "bonus".into(),
            name: "5G 赠送流量".into(),
            total_mb: 5120.0,
            remaining_mb: 5120.0,
            ..Default::default()
        });
        let report = build_report(
            &account(),
            &current,
            Some(&previous),
            Some(&previous),
            "modern-get",
        );
        assert!(report.should_notify);

        let Message { segments, .. } = report.notification;
        let [oxidebot::core::source::message::MessageSegment::RichText(value)] =
            segments.as_slice()
        else {
            panic!("notification must retain portable rich-text formatting");
        };
        let lines = value.text.lines().collect::<Vec<_>>();
        assert_eq!(lines[0], "🆕 主卡  ·  近 5 分钟");
        assert_eq!(lines[1], "新增 1 个流量包");
        assert_eq!(lines[2], "余量  通用 5.9GB  ·  免流 0MB");
    }
}
