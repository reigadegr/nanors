use crate::{Command, Error, Result, TelegramBot};
use teloxide::{requests::Requester, types::Message};
use tracing::info;

/// Handle bot commands
pub async fn handle_command(bot: TelegramBot, msg: Message, cmd: Command) -> Result<()> {
    let chat_id = msg.chat.id.0;
    let username = msg
        .from
        .as_ref()
        .and_then(|u| u.username.as_deref())
        .unwrap_or("unknown");

    match cmd {
        Command::Start => {
            info!("[@{username}] Command: /start");
            bot.bot
                .send_message(msg.chat.id, Command::welcome_text())
                .await?;
        }
        Command::Reset => {
            info!("[@{username}] Command: /reset");
            bot.reset_session(chat_id).await?;
            bot.bot.send_message(msg.chat.id, "对话历史已重置").await?;
        }
        Command::Help => {
            info!("[@{username}] Command: /help");
            bot.bot
                .send_message(msg.chat.id, Command::help_text())
                .await?;
        }
    }

    Ok(())
}

/// Handle any message (commands or regular text)
pub async fn handle_message(bot: TelegramBot, msg: Message) -> Result<()> {
    let chat_id = msg.chat.id.0;
    let text = msg.text().ok_or(Error::Config("No text content".into()))?;
    let username = msg
        .from
        .as_ref()
        .and_then(|u| u.username.as_deref())
        .unwrap_or("unknown");

    // Check if this is a command
    if let Some(cmd) = Command::parse_from_text(text, "") {
        return handle_command(bot, msg, cmd).await;
    }

    info!("[@{username}] Message: {text}");

    // Show typing indicator
    bot.bot
        .send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing)
        .await?;

    // Process message and get response
    let response = bot.process_message(chat_id, text.to_string()).await?;

    info!("[@{username}] Response: {response}");

    // Send response to Telegram
    bot.bot.send_message(msg.chat.id, response).await?;

    Ok(())
}
