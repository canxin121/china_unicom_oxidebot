# China Unicom OxideBot

为 OxideBot 提供中国联通多账号流量查询、凭据续期和阈值通知。版本 0.3 直接依赖 [`canxin121/china-unicom-rs`](https://github.com/canxin121/china-unicom-rs)，不再在本仓库复制联通 HTTP、Cookie 规范化、流量包解析或分类代码。

## 工作方式

- 一个 OxideBot 私聊用户可以添加任意多个联通账号；每个账号都有独立的登录凭据、查询配置、通知基线和定时任务。
- Bot 只接收 China Unicom Login 页面生成的规范四字段 JSON，不在聊天中执行短信登录、验证码或浏览器模拟。
- `china-unicom-rs` 会按凭据自动选择现代 GET 或兼容 POST 查询接口，解析通用/免流、有限/不限、结转、副卡和新增流量包。
- `token_online` 默认每 12 小时主动续期；Cookie 失效时会立即尝试续期并重试查询。
- 每个账号分别累计区间流量、今日流量，并按最长间隔、通用阈值、免流阈值或新增流量包发送通知。

本项目沿用上游 `china-unicom-rs` 的 GPL-3.0-only 许可证，详见 `LICENSE`。

## 四字段登录 JSON

先在受信任环境运行 `china-unicom-login-web`，由账号本人完成官方短信/验证码流程，然后把成功页面返回的完整 JSON 直接发送给 Bot：

```json
{
  "token_online": "...",
  "app_id": "...",
  "cookie": "ecs_token=...; ecs_acc=...",
  "captured_at": "2026-07-27T03:00:07+08:00"
}
```

Bot 要求 JSON 恰好包含这四个字段，并验证 `captured_at` 为 RFC 3339 时间。注册流程不接受拆分字段或单独 Cookie，也不会索取手机号、短信验证码或服务密码。

## 账号命令

所有命令只能在私聊使用。账号 ID 必须为 1–32 个 ASCII 字母、数字、下划线或连字符。

添加两个账号：

```text
/china_unicom account add main --name 主卡
/china_unicom account add backup --name 副卡
```

每条 `add` 命令之后，直接发送对应账号的四字段 JSON。

账号管理：

```text
/china_unicom account list
/china_unicom account login main
/china_unicom account remove backup
```

`account login` 用新的四字段 JSON 替换指定账号的登录包，并重启该账号的任务。

## 查询、配置和任务

不指定账号 ID 时，查询和任务命令作用于当前用户的全部联通账号：

```text
/china_unicom query
/china_unicom query main

/china_unicom config show
/china_unicom config show main
/china_unicom config set main

/china_unicom task status
/china_unicom task status main
/china_unicom task start
/china_unicom task start main
/china_unicom task stop
/china_unicom task stop main
```

每个账号可以独立设置：

- 规范四字段登录 JSON；
- 显示名称；
- 查询间隔，最少 60 秒；
- 最长通知间隔；
- 免流和通用流量阈值，单位 GB；
- `auto`、`modern`、`legacy` 查询模式；
- 主动续期间隔，单位小时，`0` 表示关闭。

## Telegram 集成

```rust
use anyhow::Context;
use china_unicom_oxidebot::ChinaUnicomHandler;
use telegram_bot_oxidebot::bot::TelegramBot;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = std::env::var("TELEGRAM_BOT_TOKEN")
        .context("TELEGRAM_BOT_TOKEN is not set")?;
    let telegram = TelegramBot::try_new(token, Default::default()).await?;

    oxidebot::OxideBotManager::new()
        .bot(telegram)
        .await
        .wait_handler(|sender| {
            Box::pin(async move {
                ChinaUnicomHandler::try_new(sender)
                    .await
                    .expect("failed to initialize China Unicom handler")
            })
        })
        .await
        .run_block()
        .await
}
```

仓库提供相同代码的可编译示例：

```bash
TELEGRAM_BOT_TOKEN='你的 Bot Token' cargo run --example telegram
```

## 数据与安全

数据默认保存在 `./china_unicom/data.db`：

- Unix 下数据目录设为 `0700`，数据库设为 `0600`；
- 数据库 SQL 日志关闭，避免凭据出现在日志中；
- 配置展示只输出脱敏后的 Cookie、token 和 app ID；
- 群聊中的命令会被拒绝；
- 同一用户的不同联通账号使用 `(owner, account_id)` 联合主键完全隔离；
- 每个账号的 `previous_snapshot` 和 `daily_snapshot` 独立保存。

四字段 JSON 等同于密码。不要提交到 Git、粘贴到 Issue 或转发给其他人。

## 从旧版本升级

启动 0.3 时自动创建 `unicom_account` 和 `unicom_account_state`：

- 0.2 及更早版本的单账号配置会迁移为账号 ID `default`；
- 原 Cookie、`token_online`、`app_id`、阈值、查询间隔、任务状态和快照都会保留；
- 旧表不会立即删除，便于恢复；新版业务只读写多账号表；
- 迁移使用 `INSERT OR IGNORE`，重复启动不会复制账号。

迁移后的旧账号建议使用以下命令导入新的规范登录包，以补充准确的 `captured_at`：

```text
/china_unicom account login default
```

## 依赖

- Rust 1.97+
- `china-unicom-rs` 1.0.0，直接 Git 依赖并由 `Cargo.lock` 固定提交
- `oxidebot` 0.1.8
- `telegram_bot_oxidebot` 0.1.4
- Tokio 1.53.1
- SeaORM / SeaORM Migration 2.0.0
- Clap 4.6.4

由于 `china-unicom-rs` 当前只发布在 GitHub、尚未发布到 crates.io，本项目设置为 `publish = false`；请从 Git 仓库构建或作为 Git 依赖使用。

## 验证

```bash
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```
