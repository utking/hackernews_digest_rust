use crate::{
    sender::{DummySender, Sender, SmtpSender, TelegramSender},
    Deserialize, ItemFilter, Serialize,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SmtpConfig {
    pub from: String,
    pub to: String,
    pub host: String,
    pub password: String,
    pub port: u16,
    pub subject: String,
    pub use_ssl: bool,
    pub use_tls: bool,
    pub username: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelegramConfig {
    pub token: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RssSource {
    pub url: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub blacklisted_domains: Vec<String>,
    pub db_dsn: String,
    pub filters: Vec<ItemFilter>,
    pub purge_after_days: i64,
    pub smtp: Option<SmtpConfig>,
    pub telegram: Option<TelegramConfig>,
    pub rss_sources: Option<Vec<RssSource>>,
}

impl AppConfig {
    pub fn from_file(file_name: &String) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(file_name)?;
        let config: AppConfig = serde_json::from_str(&contents)?;

        Ok(config)
    }

    #[allow(dead_code)]
    pub fn from_str(contents: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config: AppConfig = serde_json::from_str(contents)?;

        Ok(config)
    }

    pub fn get_sender(&self) -> Sender {
        if let Some(config) = &self.smtp {
            Sender::Smtp(SmtpSender::new(config))
        } else if let Some(config) = &self.telegram {
            Sender::Telegram(TelegramSender::new(config))
        } else {
            Sender::Dummy(DummySender {})
        }
    }
}
