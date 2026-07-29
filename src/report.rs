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
    pub message: Message,
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
    report.bold("流量概览");
    report.line();
    usage_line(&mut report, "通用", normal.used_mb, normal.remaining_mb);
    usage_line(&mut report, "免流", free.used_mb, free.remaining_mb);

    report.line();
    report.bold("用量变化");
    report.line();
    report.text("本次  通用 ");
    report.code(format_data_mb(normal.interval_used_mb));
    report.text("  ·  免流 ");
    report.code(format_data_mb(free.interval_used_mb));
    report.line();
    report.text("今日  通用 ");
    report.code(format_data_mb(normal.today_used_mb));
    report.text("  ·  免流 ");
    report.code(format_data_mb(free.today_used_mb));
    report.line();

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
        message: report.into_message(),
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
    fn compact_report_uses_rich_text_and_preserves_threshold_behavior() {
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
        let Message { segments, .. } = report.message;
        let [oxidebot::core::source::message::MessageSegment::RichText(value)] =
            segments.as_slice()
        else {
            panic!("report must retain portable rich-text formatting");
        };
        assert!(value.text.contains("流量概览"));
        assert!(value.text.contains("套餐明细"));
        assert!(!value.spans.is_empty());
        assert!(value.text.lines().count() < 20);
    }
}
