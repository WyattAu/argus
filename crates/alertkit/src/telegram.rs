//! Telegram delivery sink.

use std::time::Duration;

use feedkit::Alert;

const SEND_URL: &str = "https://api.telegram.org";

/// Delivers alerts to a Telegram chat via the bot API. Failures log at warn
/// and never propagate — alerting must not take the pipeline down.
#[derive(Clone)]
pub struct TelegramSink {
    client: fetch_kit::Client,
    bot_token: String,
    chat_id: String,
}

impl TelegramSink {
    /// Sink for the given bot token + chat id.
    #[must_use]
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self {
            client: fetch_kit::Client::builder()
                .user_agent("argus-alertkit/0.1")
                .timeout(Duration::from_secs(10))
                .retries(2)
                .build(),
            bot_token,
            chat_id,
        }
    }

    /// Sends one alert. Fire-and-forget: errors are logged, not returned.
    pub async fn send(&self, alert: &Alert) {
        let url = format!("{SEND_URL}/bot{}/sendMessage", self.bot_token);
        let body = serde_json::json!({
            "chat_id": self.chat_id,
            "text": format!("argus · {}\n{}", alert.rule_name, alert.message),
        });
        match self.client.post(url).json(&body).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    tracing::warn!(status = %response.status(), "telegram send rejected");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "telegram send failed");
            }
        }
    }
}
