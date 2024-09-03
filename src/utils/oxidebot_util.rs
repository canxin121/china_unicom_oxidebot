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

// pub type HandleConfirmFn = Box<
//     dyn Fn(&sea_orm::DatabaseConnection, Matcher) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>
//         + Send
//         + Sync,
// >;

// pub struct ConfirmInteraction {
//     db: sea_orm::DatabaseConnection,
//     handle_confirm: HandleConfirmFn,
// }

// impl Drop for ConfirmInteraction {
//     fn drop(&mut self) {
//         tracing::error!("Dropping ConfirmInteraction");
//     }
// }

// impl ConfirmInteraction {
//     pub fn new(
//         db: sea_orm::DatabaseConnection,
//         handle_confirm: HandleConfirmFn,
//     ) -> std::sync::Arc<Interaction<ConfirmInteraction>> {
//         Interaction::new(Self {
//             db,
//             handle_confirm: Box::new(handle_confirm),
//         })
//     }
// }

// impl InteractionTrait for ConfirmInteraction {
//     #[must_use]
//     #[allow(
//         elided_named_lifetimes,
//         clippy::type_complexity,
//         clippy::type_repetition_in_bounds
//     )]
//     fn should_start<'life0, 'async_trait>(
//         &'life0 self,
//         matcher: Matcher,
//     ) -> ::core::pin::Pin<
//         Box<dyn ::core::future::Future<Output = bool> + ::core::marker::Send + 'async_trait>,
//     >
//     where
//         'life0: 'async_trait,
//         Self: 'async_trait,
//     {
//         Box::pin(async move {
//             return true;
//         })
//     }

//     #[must_use]
//     #[allow(
//         elided_named_lifetimes,
//         clippy::type_complexity,
//         clippy::type_repetition_in_bounds
//     )]
//     fn handle_interaction<'a, 'async_trait>(
//         &'a self,
//         init_matcher: Matcher,
//         mut receiver: tokio::sync::mpsc::Receiver<Matcher>,
//     ) -> ::core::pin::Pin<
//         Box<dyn ::core::future::Future<Output = Result<()>> + ::core::marker::Send + 'async_trait>,
//     >
//     where
//         'a: 'async_trait,
//         Self: 'async_trait,
//     {
//         Box::pin(async move {
//             init_matcher
//                 .try_send_message(vec![MessageSegment::text(
//                     "Are you sure to continue? (y/n)",
//                 )])
//                 .await?;

//             let matcher = wait_for_input!(&mut receiver)?;

//             if let Some(message) = matcher.try_get_message() {
//                 let raw_text = message.get_raw_text();
//                 if raw_text.to_lowercase() == "y" || raw_text.to_lowercase() == "yes" {
//                     (self.handle_confirm)(&self.db, matcher.clone()).await;
//                     return Ok(());
//                 }
//             }

//             matcher
//                 .try_send_message(vec![MessageSegment::text("Cancelled")])
//                 .await?;

//             Ok(())
//         })
//     }
// }
