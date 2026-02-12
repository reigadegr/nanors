use teloxide::types::BotCommand;

#[derive(Clone, Debug)]
pub enum Command {
    Start,
    Reset,
    Help,
}

impl Command {
    fn all() -> Vec<BotCommand> {
        vec![
            BotCommand {
                command: "start".to_string(),
                description: "开始使用机器人".to_string(),
            },
            BotCommand {
                command: "reset".to_string(),
                description: "重置对话历史".to_string(),
            },
            BotCommand {
                command: "help".to_string(),
                description: "显示帮助信息".to_string(),
            },
        ]
    }

    #[must_use]
    pub fn bot_commands() -> Vec<BotCommand> {
        Self::all()
    }

    #[must_use]
    pub fn parse_from_text(text: &str, _bot_name: &str) -> Option<Self> {
        let text = text.trim().to_lowercase();

        // Remove bot mention if present (e.g., "/start@my_bot")
        let text = text.split('@').next().unwrap_or(&text).to_string();

        match text.as_str() {
            "/start" => Some(Self::Start),
            "/reset" => Some(Self::Reset),
            "/help" => Some(Self::Help),
            _ => None,
        }
    }

    #[must_use]
    pub const fn help_text() -> &'static str {
        r"
🤖 NanoRS Telegram Bot

命令列表:
/start - 开始使用机器人
/reset - 重置对话历史
/help  - 显示此帮助信息

直接发送消息即可开始对话！
"
    }

    #[must_use]
    pub const fn welcome_text() -> &'static str {
        r"
👋 欢迎使用 NanoRS Telegram Bot！

我是您的 AI 助手，可以:
• 回答问题
• 管理对话记忆
• 提供智能建议

发送 /help 查看命令列表。
"
    }
}
