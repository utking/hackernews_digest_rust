use regex::Regex;
use rss::Channel;

use crate::{
    common::FetchOperation, config::AppConfig, establish_connection, run_migrations, AnyConnection,
    DigestItem, Fetch, Filters,
};

use super::prelude::FeedItem;

pub struct RssFetcher {
    config: AppConfig,
    filters: Vec<Regex>,
}

impl RssFetcher {
    /// Create a new HabrFetcher
    #[must_use]
    pub fn new(config: &AppConfig) -> RssFetcher {
        Self {
            config: config.clone(),
            filters: Filters::compile(config.filters.clone()),
        }
    }

    async fn pull_feed_items(
        &self,
        source_url: &str,
        reverse: bool,
    ) -> Result<Vec<DigestItem>, Box<dyn std::error::Error>> {
        let content = reqwest::get(source_url).await?.bytes().await?;
        let channel = Channel::read_from(&content[..])?;
        let news_items: Vec<FeedItem> = channel
            .items()
            .iter()
            .map(|item| FeedItem::from(item))
            .collect();

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
        source_url: &str,
        reverse: bool,
        mut _conn: &mut AnyConnection,
    ) -> Result<Vec<DigestItem>, Box<dyn std::error::Error>> {
        let digest = self.pull_feed_items(source_url, reverse).await?;

        // Store the news items in the database
        // crate::store_news_items(&digest, &mut conn)?;

        Ok(digest)
    }

    /// Keep an item based on the filters. If reverse is true, keep the item if it doesn't match
    fn keep_item(&self, title: &String, reverse: bool) -> bool {
        let mut keep: bool = false;
        if reverse {
            for filter in &self.filters {
                if !filter.is_match(title) {
                    keep = true;
                    break;
                }
            }
        } else {
            for filter in &self.filters {
                if filter.is_match(title) {
                    keep = true;
                    break;
                }
            }
        }
        keep
    }

    async fn vacuum(&self, conn: &mut AnyConnection) -> Result<i32, Box<dyn std::error::Error>> {
        let num_deleted = crate::vacuum(self.config.purge_after_days as i32, conn)?;

        Ok(num_deleted as i32)
    }
}

impl Fetch for RssFetcher {
    async fn run(&self, op: &FetchOperation) -> Result<i32, Box<dyn std::error::Error>> {
        let mut conn = establish_connection(&self.config.db_dsn);
        let conn_arg = &mut conn;
        match run_migrations(conn_arg) {
            Ok(()) => {}
            Err(e) => eprintln!("Error running migrations: {e}"),
        }

        match op {
            FetchOperation::Fetch(reverse) => {
                let mut total_fetched = 0;
                for source in self.config.rss_sources.clone().unwrap_or(vec![]) {
                    let digest = self.fetch(&source.url, *reverse, conn_arg).await?;
                    // Send an email with the digest if it's not empty
                    if !digest.is_empty() {
                        // send the digest to the email address in the config, if given
                        self.config.get_sender().send_digest(&digest).await?;
                        total_fetched += digest.len();
                    }
                }
                Ok(total_fetched as i32)
            }
            FetchOperation::Vacuum => self.vacuum(conn_arg).await,
        }
    }
}
