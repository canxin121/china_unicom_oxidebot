use oxidebot::{bot::get_bot, matcher::Matcher, source::message::MessageSegment};

pub async fn get_user_bot_from(matcher: &Matcher) -> Option<(String, String)> {
    let user = format!("{}_{}", matcher.bot.server(), matcher.try_get_user()?.id);
    let bot = format!(
        "{}_{}",
        matcher.bot.server(),
        matcher.bot.bot_info().await.id?
    );
    Some((user, bot))
}

pub async fn send_message(user: &str, bot: &str, message: String) -> anyhow::Result<()> {
    let (server, bot_id) = bot.split_once("_").ok_or(anyhow::anyhow!("Invalid bot"))?;
    let (_, user_id) = user
        .split_once("_")
        .ok_or(anyhow::anyhow!("Invalid user"))?;

    let bot = get_bot(server, bot_id)
        .await
        .ok_or(anyhow::anyhow!("Bot not found"))?;

    bot.send_message(
        vec![MessageSegment::text(message)],
        oxidebot::api::payload::SendMessageTarget::Private(user_id.to_string()),
    )
    .await?;
    Ok(())
}
