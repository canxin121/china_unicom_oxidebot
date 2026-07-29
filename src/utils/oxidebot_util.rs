//! Stable account ownership keys and proactive delivery helpers.

use anyhow::{Context, Result};
use oxidebot::{
    BotId, BotIdentity, PlatformId,
    delivery::{Address, BotDirectory, BotSelection, FallbackPolicy},
};

/// Returns the historical `platform_identifier` key used by the persisted schema.
///
/// Keeping this format means existing 0.1 account rows continue to belong to
/// the same person after the runtime migration. Platform IDs supplied by
/// official OxideBot adapters are semantic identifiers and never contain `_`.
pub fn participant_key(identity: &BotIdentity, identifier: &str) -> String {
    format!("{}_{}", identity.platform.as_str(), identifier)
}

/// Delivers a scheduled notification through the bot saved with the account.
pub async fn send_message(
    bots: &BotDirectory,
    user: &str,
    stored_bot: &str,
    message: String,
) -> Result<()> {
    let (platform, bot_id) = stored_bot
        .split_once('_')
        .context("stored bot identity has no platform separator")?;
    let (_, user_id) = user
        .split_once('_')
        .context("stored account owner has no platform separator")?;
    let platform = PlatformId::new(platform.to_owned()).context("stored platform ID is invalid")?;
    let bot_id = BotId::new(bot_id.to_owned()).context("stored bot ID is invalid")?;
    bots.send_address(
        Address::direct(user_id).through(BotSelection::Exact(BotIdentity::new(platform, bot_id))),
        message,
        FallbackPolicy::Auto,
    )
    .await
    .context("scheduled China Unicom notification could not be delivered")?;
    Ok(())
}
