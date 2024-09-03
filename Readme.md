# ChinaUnicom Oxidebot

A chatbot cli to actively check or receive scheduled/threshold notifications about China Unicom flow usage.

### example usage

You need to use [Oxidebot](https://github.com/canxin121/oxidebot) with this handler.

```rust
use china_unicom_oxidebot::ChinaUnicomHandler;
use telegram_bot_oxidebot::bot::TelegramBot;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    oxidebot::OxideBotManager::new()
        .bot(
            TelegramBot::new(
                "token".to_string(),
                Default::default(),
            )
            .await,
        )
        .await
        .handler(ChinaUnicomHandler::new().await)
        .run_block()
        .await;
}
```