use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub guid: String,
    pub created_at: i64,
    pub categories: Vec<String>,
}

impl FeedItem {
    pub fn from(item: &rss::Item) -> FeedItem {
        let categories = item
            .categories()
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        let guid = match item.guid() {
            Some(g) => g.value().to_string(),
            None => "".to_string(),
        };
        let id = guid
            .trim_end_matches("/")
            .split("/")
            .last()
            .unwrap_or_default()
            .parse()
            .unwrap_or_default();
        Self {
            id,
            guid: guid,
            title: item.title().unwrap_or("").to_string(),
            description: item.description().unwrap_or("").to_string(),
            created_at: 0,
            categories,
        }
    }
}
