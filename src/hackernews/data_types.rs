use crate::DigestItem;
use serde::Deserialize;

/// List of news items that have been fetched since the last fetch
pub type Digest = Vec<DigestItem>;

/// A news item that has been fetched from the API
#[derive(Debug, Deserialize)]
pub struct JsonNewsItem {
    id: i64,
    title: Option<String>,
    url: Option<String>,
    time: i64,
}

impl JsonNewsItem {
    pub fn as_digest_item(&self) -> DigestItem {
        let mut item = DigestItem {
            id: self.id as i32,
            news_title: self.title.clone().unwrap_or_default(),
            news_url: self.url.clone().unwrap_or_default(),
            created_at: self.time as i32,
        };

        if item.news_url.is_empty() {
            item.news_url = String::from("-");
            item.news_title = String::from("-");
        }

        item
    }
}
