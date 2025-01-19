use serde::{Deserialize, Serialize};

use crate::ItemFilter;

pub const API_BASE_URL: &str = "https://hacker-news.firebaseio.com/v0";

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

pub enum SenderType {
    Email(SmtpConfig),
    Telegram(TelegramConfig),
    Console,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelegramConfig {
    pub token: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub blacklisted_domains: Vec<String>,
    pub db_dsn: String,
    pub filters: Vec<ItemFilter>,
    pub purge_after_days: u64,
    pub smtp: Option<SmtpConfig>,
    pub telegram: Option<TelegramConfig>,
}

impl AppConfig {
    pub fn from_file(file_name: &String) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(file_name)?;
        let config: AppConfig = serde_json::from_str(&contents)?;

        Ok(config)
    }

    #[allow(dead_code)]
    pub fn from_str(contents: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config: AppConfig = serde_json::from_str(&contents)?;

        Ok(config)
    }

    pub fn get_sender(&self) -> SenderType {
        if let Some(config) = &self.smtp {
            SenderType::Email(config.clone())
        } else if let Some(config) = &self.telegram {
            SenderType::Telegram(config.clone())
        } else {
            SenderType::Console
        }
    }
}
