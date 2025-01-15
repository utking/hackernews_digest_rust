use serde::{Deserialize, Serialize};

use crate::ItemFilter;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SmtpConfig {
    pub from: String,
    pub host: String,
    pub password: String,
    pub port: u16,
    pub subject: String,
    pub use_ssl: bool,
    pub use_tls: bool,
    pub username: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub api_base_url: String,
    pub blacklisted_domains: Vec<String>,
    pub db_dsn: String,
    pub email_to: Option<String>,
    pub filters: Vec<ItemFilter>,
    pub purge_after_days: u64,
    pub smtp: Option<SmtpConfig>,
}

impl AppConfig {
    pub fn from_file(file_name: &String) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(file_name)?;
        let config: AppConfig = serde_json::from_str(&contents)?;

        Ok(config)
    }
}
