use anyhow::Context;
use china_unicom_oxidebot::ChinaUnicomHandler;
use telegram_bot_oxidebot::bot::TelegramBot;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = std::env::var("TELEGRAM_BOT_TOKEN").context("TELEGRAM_BOT_TOKEN is not set")?;
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
