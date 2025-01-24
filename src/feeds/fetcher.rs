use regex::Regex;
use rss::Channel;

use crate::{
    config::{AppConfig, RssSource},
    establish_connection, run_migrations, AnyConnection, DigestItem, Fetch, Filters,
};

use super::prelude::FeedItem;

pub struct RssFetcher {
    config: AppConfig,
    filters: Vec<Regex>,
}

impl RssFetcher {
    /// Create a new fetcher
    #[must_use]
    pub fn new(config: &AppConfig) -> RssFetcher {
        Self {
            config: config.clone(),
            filters: Filters::compile(&config.filters),
        }
    }

    async fn pull_feed_items(
        &self,
        source_url: &str,
        reverse: bool,
    ) -> Result<Vec<DigestItem>, Box<dyn std::error::Error>> {
        let content = reqwest::get(source_url).await?.bytes().await?;
        let channel = Channel::read_from(&content[..])?;
        let news_items: Vec<FeedItem> = channel.items().iter().map(FeedItem::from).collect();

        let mut items = Vec::new();
        for item in news_items {
            if self.keep_item(&item.title.clone(), reverse) {
                items.push(DigestItem {
                    id: item.id as i32,
                    news_title: item.title,
                    news_url: item.guid,
                    created_at: item.created_at as i32,
                });
            }
        }

        Ok(items)
    }

    /// Fetch the latest news from the Habr API
    async fn fetch(
        &self,
        source: &RssSource,
        reverse: bool,
        mut conn: &mut AnyConnection,
    ) -> Result<Vec<DigestItem>, Box<dyn std::error::Error>> {
        let mut digest = Vec::new();
        let prefetched_items = self.pull_feed_items(&source.url, reverse).await?;
        let items_ids: Vec<i32> = prefetched_items.iter().map(|item| item.id).collect();

        // Get the items that are not already in the database
        let ids_to_pull = crate::get_ids_to_pull(items_ids, &mut conn);
        // Compile a digest from the items that are not in the database yet
        for id in ids_to_pull {
            let item = prefetched_items.iter().find(|item| item.id == id);
            if let Some(item) = item {
                digest.push(item.clone());
            }
        }

        // Store the news items in the database
        crate::store_feed_items(&source.name, &digest, &mut conn)?;

        Ok(digest)
    }

    /// Keep an item based on the filters. If reverse is true, keep the item if it doesn't match
    fn keep_item(&self, title: &str, reverse: bool) -> bool {
        let keep: bool = reverse;
        for filter in &self.filters {
            if filter.is_match(title) {
                return !reverse;
            }
        }
        keep
    }
}

impl Fetch for RssFetcher {
    async fn run(&self, reverse: bool) -> Result<usize, Box<dyn std::error::Error>> {
        let mut conn = establish_connection(&self.config.db_dsn);
        let conn_arg = &mut conn;
        match run_migrations(conn_arg) {
            Ok(()) => {}
            Err(e) => eprintln!("Error running migrations: {e}"),
        }

        let mut total_fetched = 0;
        for source in self.config.rss_sources.clone().unwrap_or_default() {
            let digest = self.fetch(&source, reverse, conn_arg).await?;
            // Send an email with the digest if it's not empty
            if !digest.is_empty() {
                // send the digest to the email address in the config, if given
                self.config
                    .get_sender()
                    .send_digest(&source.name, &digest)
                    .await?;
                total_fetched += digest.len();
            }
        }
        Ok(total_fetched)
    }
}

#[cfg(test)]
mod test {
    use super::{AppConfig, RssFetcher};
    use crate::{feeds::prelude::FeedItem, ItemFilter};
    use tokio::test;

    #[test]
    async fn test_filter_fetched() {
        let config = AppConfig {
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust,python".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            rss_sources: None,
            purge_after_days: 7,
            blacklisted_domains: vec![],
        };

        // Filter with direct filtering first
        let mut reverse = false;

        let fetcher = RssFetcher::new(&config);
        let pulled_items: Vec<FeedItem> = vec![
            FeedItem {
                id: 123,
                title: "Python is a programming language".to_string(),
                guid: "https://example.com/items/123".to_string(),
                created_at: 0,
                description: String::from("Some description"),
                categories: vec![String::from("Python")],
            },
            FeedItem {
                id: 202,
                title: "Rust is cool".to_string(),
                guid: "https://example.com/items/202".to_string(),
                created_at: 0,
                description: String::from("Some description"),
                categories: vec![String::from("Rust")],
            },
            FeedItem {
                id: 303,
                title: "1C is not cool".to_string(),
                guid: "https://example.com/items/303".to_string(),
                created_at: 0,
                description: String::from("Some description"),
                categories: vec![String::from("1C")],
            },
        ];

        assert_eq!(
            pulled_items
                .iter()
                .filter(|i| fetcher.keep_item(&i.title.clone(), reverse))
                .count(),
            2,
            "Filter/keep check failed",
        );

        eprintln!("\n\n\n");

        // Filter with reverse filtering
        reverse = true;
        assert_eq!(
            pulled_items
                .iter()
                .filter(|i| fetcher.keep_item(&i.title.clone(), reverse))
                .count(),
            1,
            "Reverse filter/keep check failed",
        );
    }
}
