use crate::DigestItem;
use serde::Deserialize;

/// A news item that has been fetched from the API
#[derive(Debug, Deserialize)]
pub struct JsonNewsItem {
    id: i64,
    title: Option<String>,
    url: Option<String>,
    time: i64,
}

impl Into<DigestItem> for JsonNewsItem {
    fn into(self) -> DigestItem {
        let mut item = DigestItem {
            id: self.id as _,
            news_title: self.title.clone().unwrap_or_default(),
            news_url: self.url.clone().unwrap_or_default(),
            created_at: self.time as _,
        };

        if item.news_url.is_empty() {
            item.news_url = String::from("-");
            item.news_title = String::from("-");
        }

        item
    }
}
