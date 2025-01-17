use crate::{
    digest_to_html, digest_to_text, establish_connection, schemas::prelude::run_migrations,
    store_news_item, AppConfig, Digest, JsonNewsItem, Sender,
};
use diesel::SqliteConnection;
use regex::{Regex, RegexBuilder};
use url::Url;

pub enum FetchOperation {
    Fetch(bool),
    Vacuum,
}

pub struct Fetcher {
    pub config: AppConfig,
    filters: Vec<Regex>,
}

impl Fetcher {
    /// Create a new fetcher with the given configuration
    pub fn new(config: &AppConfig) -> Self {
        // for filter in &config.filters, split the "value" field by comma and store in a vector
        let string_filters: Vec<String> = config
            .filters
            .iter()
            .map(|f| f.value.split(',').collect::<Vec<&str>>())
            .flatten()
            .map(|s| s.to_string())
            .collect();

        let mut filters: Vec<Regex> = Vec::new();
        for filter in string_filters {
            match RegexBuilder::new(&filter.to_lowercase())
                .case_insensitive(true)
                .build()
            {
                Ok(re) => filters.push(re),
                Err(e) => eprintln!("Error creating filter: {}", e),
            }
        }

        Self {
            config: config.clone(),
            filters,
        }
    }

    /// Run the fetcher with the given operation. The operation can be either fetching
    /// new news items or vacuuming the database. Return the number of items fetched.
    /// If digest is not empty, send an email with the digest to the email address in the config.
    pub async fn run(&self, op: &FetchOperation) -> Result<i32, Box<dyn std::error::Error>> {
        let mut conn = establish_connection(&self.config.db_dsn);
        let conn_arg = &mut conn;
        match run_migrations(conn_arg) {
            Ok(_) => {}
            Err(e) => eprintln!("Error running migrations: {}", e),
        }

        match op {
            FetchOperation::Fetch(reverse) => {
                let digest = self.fetch(*reverse, conn_arg).await?;
                // Send an email with the digest if it's not empty
                if digest.len() > 0 {
                    // send the digest to the email address in the config, if given
                    match self.config.email_to {
                        Some(ref send_to) => {
                            if !send_to.is_empty() {
                                match self.config.smtp {
                                    Some(ref smtp_config) => {
                                        let sender = Sender::new(smtp_config);
                                        let text_body = digest_to_text(&digest);
                                        let html_body = digest_to_html(&digest);
                                        sender
                                            .send_email(
                                                &send_to,
                                                smtp_config.subject.as_str(),
                                                &text_body,
                                                &html_body,
                                            )
                                            .await?;
                                    }
                                    None => {}
                                }
                            }
                        }
                        None => {}
                    }
                }
                Ok(digest.len() as i32)
            }
            FetchOperation::Vacuum => self.vacuum(conn_arg).await,
        }
    }

    /// Fetch not previously fetched news items from the API. For that, we need to:
    /// 1. Fetch the top stories' IDs from the API
    /// 2. Fetch each news item by its ID if it wasn't previously fetched; existing
    ///   news items' IDs are stored in the database
    /// 3. Apply filters to each news item
    /// 4. Store the news items in the database
    /// 5. Return the digest of new items fetched
    async fn fetch(
        &self,
        reverse: bool,
        mut conn: &mut SqliteConnection,
    ) -> Result<Digest, Box<dyn std::error::Error>> {
        let mut digest: Digest = Vec::new();

        let prefetched = self.prefetch().await?;
        let ids_to_pull = crate::get_ids_to_pull(prefetched, conn);

        for id in ids_to_pull[0..ids_to_pull.len()].iter() {
            let news_item = &self.fetch_news_item(*id).await?;
            let digest_item = &news_item.as_digest_item();

            // Skip blacklisted domains, but store the news item in the database
            if self.is_blacklisted(&digest_item.news_url) {
                eprintln!("Skipping blacklisted domain: {}", digest_item.news_url);
                store_news_item(digest_item, &mut conn)?;
                continue;
            }

            // Skip items with missing URLs from the digest, but store them in the database
            if self.is_missing_url(&digest_item.news_url) {
                eprintln!("Skipping item with missing URL: {}", digest_item.news_title);
                store_news_item(digest_item, &mut conn)?;
                continue;
            }

            // Apply filters
            if !self.keep_item(&digest_item.news_title.clone(), reverse) {
                store_news_item(digest_item, &mut conn)?;
                eprintln!("Skipping filtered item: {}", digest_item.news_title);
                continue;
            }

            digest.push(digest_item.clone());
        }

        eprintln!("Storing {} news items in the database", digest.len());

        // Store the news items in the database
        crate::store_digest(&digest, &mut conn)?;

        Ok(digest)
    }

    /// Keep an item based on the filters. If reverse is true, keep the item if it doesn't match
    fn keep_item(&self, title: &String, reverse: bool) -> bool {
        let mut keep: bool = false;
        if reverse {
            for filter in &self.filters {
                if !filter.is_match(&title) {
                    eprintln!("Keeping item for reverse: {}", title);
                    keep = true;
                    break;
                }
            }
        } else {
            for filter in &self.filters {
                if filter.is_match(&title) {
                    eprintln!("Keeping item: {}", title);
                    keep = true;
                    break;
                }
            }
        }
        keep
    }

    /// Vacuum the database - remove old news items
    async fn vacuum(&self, conn: &mut SqliteConnection) -> Result<i32, Box<dyn std::error::Error>> {
        let num_deleted = crate::vacuum(self.config.purge_after_days as i32, conn)?;

        Ok(num_deleted as i32)
    }

    fn is_missing_url(&self, item_url: &String) -> bool {
        item_url.is_empty() || item_url == "-"
    }

    /// Fetch the top stories' IDs from the API
    async fn prefetch(&self) -> Result<Vec<i32>, Box<dyn std::error::Error>> {
        let result = reqwest::get(format!("{}/topstories.json", self.config.api_base_url))
            .await?
            .json::<Vec<i32>>()
            .await?;

        Ok(result)
    }

    /// Fetch a single news item by its ID
    async fn fetch_news_item(&self, id: i32) -> Result<JsonNewsItem, Box<dyn std::error::Error>> {
        let result = reqwest::get(format!("{}/item/{id}.json", self.config.api_base_url))
            .await?
            .json::<JsonNewsItem>()
            .await?;

        Ok(result)
    }

    /// Check if a URL's domain is in the blacklist
    fn is_blacklisted(&self, url: &String) -> bool {
        if url.is_empty() {
            return false;
        }

        match Url::parse(url) {
            Ok(parsed_url) => match parsed_url.domain() {
                Some(domain) => {
                    for blacklisted_domain in &self.config.blacklisted_domains {
                        if domain == blacklisted_domain {
                            return true;
                        }
                    }
                }
                None => return false,
            },
            Err(_) => return false,
        }

        false
    }
}

#[cfg(test)]
mod test {
    use crate::{schemas::prelude::run_migrations, DigestItem, ItemFilter};
    use tokio::test;

    #[test]
    async fn test_is_empty_url() {
        let pulled_items = vec![
            DigestItem {
                news_title: "Rust is awesome".to_string(),
                news_url: "https://example.com".to_string(),
                created_at: 1700000000,
                id: 1,
            },
            DigestItem {
                news_title: "Missing URL".to_string(),
                news_url: "".to_string(),
                created_at: 1700000000,
                id: 2,
            },
            DigestItem {
                news_title: "".to_string(),
                news_url: "".to_string(),
                created_at: 1700000000,
                id: 3,
            },
        ];
        let config = crate::AppConfig {
            api_base_url: "https://localhost/v0".to_string(),
            db_dsn: ":memory:".to_string(),
            email_to: None,
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let fetcher = crate::Fetcher::new(&config);

        assert_eq!(
            pulled_items
                .iter()
                .filter(|i| fetcher.is_missing_url(&i.news_url))
                .count(),
            2,
            "Missing URL check failed",
        );
    }

    #[test]
    async fn test_is_blacklisted() {
        let pulled_items = vec![
            DigestItem {
                news_title: "Rust is awesome".to_string(),
                news_url: "https://example.com".to_string(),
                created_at: 1700000000,
                id: 1,
            },
            DigestItem {
                news_title: "Rust is awesome".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 2,
            },
        ];
        let config = crate::AppConfig {
            api_base_url: "https://localhost/v0".to_string(),
            db_dsn: ":memory:".to_string(),
            email_to: None,
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let fetcher = crate::Fetcher::new(&config);

        assert_eq!(
            pulled_items
                .iter()
                .filter(|i| fetcher.is_blacklisted(&i.news_url))
                .count(),
            1,
            "Blacklisted domain check failed",
        );
    }

    #[test]
    async fn test_prefetch() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let expected_addr_str = format!("http://127.0.0.1:{}", server.port());
        let prefetch_mock = server.mock(|when, then| {
            when.method(GET).path("/topstories.json");
            then.status(200)
                .header("content-type", "application/json")
                .body("[1, 2, 3, 4, 5]");
        });

        let config = crate::AppConfig {
            api_base_url: expected_addr_str,
            db_dsn: ":memory:".to_string(),
            email_to: None,
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let fetcher = crate::Fetcher::new(&config);

        let ids = fetcher.prefetch().await.unwrap();
        prefetch_mock.assert();
        assert!(!ids.is_empty(), "Prefetch failed");
        assert_eq!(ids.len(), 5, "Prefetch failed");
    }

    #[test]
    async fn test_fetch_news_item() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let expected_addr_str = format!("http://127.0.0.1:{}", server.port());
        let prefetch_mock = server.mock(|when, then| {
            when.method(GET).path("/item/111.json");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "by": "thatxliner",
                        "descendants": 10,
                        "id": 111,
                        "kids": [42707390, 42722438, 42706717, 42708236],
                        "score": 12,
                        "text": "If so, how was it like? What happened?",
                        "time": 1736904177,
                        "title": "Ask HN: Have any of you become homeless?",
                        "type": "story"
                    }"#,
                );
        });

        let config = crate::AppConfig {
            api_base_url: expected_addr_str,
            db_dsn: ":memory:".to_string(),
            email_to: None,
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };

        let fetcher = crate::Fetcher::new(&config);
        let item = fetcher.fetch_news_item(111).await.unwrap();
        prefetch_mock.assert();
        let digest_item = item.as_digest_item();
        // the item is with empty URL, so the title and the URL are reset to empty
        assert_eq!(digest_item.news_title, "-");
        assert_eq!(digest_item.news_url, "-");
        assert_eq!(digest_item.created_at, 1736904177);
        assert_eq!(digest_item.id, 111);
    }

    #[test]
    /// Puts some items in the database and then fetches the IDs to pull
    /// from the database. The IDs to pull from the API should be the
    /// difference between the prefetched IDs and the IDs in the database.
    async fn test_find_ids_diff() {
        let pulled_items = vec![
            DigestItem {
                news_title: "Rust is awesome".to_string(),
                news_url: "https://example.com".to_string(),
                created_at: 1700000000,
                id: 1,
            },
            DigestItem {
                news_title: "Rust is awesome".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 2,
            },
        ];
        // create a mock http server
        let server = httpmock::MockServer::start();
        let expected_addr_str = format!("http://127.0.0.1:{}", server.port());
        let prefetch_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/topstories.json");
            then.status(200)
                .header("content-type", "application/json")
                .body("[1, 2, 3, 4, 5]");
        });
        let config = crate::AppConfig {
            api_base_url: expected_addr_str,
            db_dsn: ":memory:".to_string(),
            email_to: None,
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let fetcher = crate::Fetcher::new(&config);

        let mut conn = crate::establish_connection(&config.db_dsn);
        // apply migrations
        run_migrations(&mut conn).unwrap();
        // store the pulled items in the database to have IDs to pull
        crate::store_digest(&pulled_items, &mut conn).unwrap();

        let prefetched = fetcher.prefetch().await.unwrap();
        prefetch_mock.assert();

        let ids_to_pull = crate::get_ids_to_pull(prefetched, &mut conn);
        assert_eq!(ids_to_pull.len(), 3, "Pulling IDs from DB failed");
        assert_eq!(ids_to_pull, vec![3, 4, 5], "Pulling IDs from DB failed");
    }

    #[test]
    async fn test_run_fetch() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let expected_addr_str = format!("http://127.0.0.1:{}", server.port());
        let prefetch_mock = server.mock(|when, then| {
            when.method(GET).path("/topstories.json");
            then.status(200)
                .header("content-type", "application/json")
                .body("[14, 15]");
        });

        let news_item_mock = server.mock(|when, then| {
            when.method(GET).path("/item/14.json");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "id": 14,
                        "text": "If so, how was it like? What happened?",
                        "time": 1736904177,
                        "title": "Ask HN: Have any of you become homeless?"
                    }"#,
                );
        });
        let news_item5_mock = server.mock(|when, then| {
            when.method(GET).path("/item/15.json");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "id": 15,
                        "time": 1736908019,
                        "title": "Item 15",
                        "url": "https://www.cnbc.com/2025/01/14/item15.html"
                    }"#,
                );
        });

        let config = crate::AppConfig {
            api_base_url: expected_addr_str,
            db_dsn: ":memory:".to_string(),
            email_to: None,
            filters: vec![ItemFilter {
                value: ".*".to_string(), // match all
                title: "PLs".to_string(),
            }],
            smtp: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };

        let fetcher = crate::Fetcher::new(&config);
        let op = crate::FetchOperation::Fetch(false);
        let num_fetched = fetcher.run(&op).await.unwrap();

        prefetch_mock.assert();
        news_item_mock.assert();
        news_item5_mock.assert();

        assert_eq!(num_fetched, 1, "Fetched items count is wrong");
    }

    #[test]
    async fn test_run_reverse_fetch() {
        use httpmock::prelude::*;

        let server = MockServer::start();
        let expected_addr_str = format!("http://127.0.0.1:{}", server.port());
        let prefetch_mock = server.mock(|when, then| {
            when.method(GET).path("/topstories.json");
            then.status(200)
                .header("content-type", "application/json")
                .body("[14, 15]");
        });

        let news_item_mock = server.mock(|when, then| {
            when.method(GET).path("/item/14.json");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "id": 14,
                        "url": "https://example.com/14",
                        "time": 1736904177,
                        "title": "Ask HN: Have any of you become homeless?"
                    }"#,
                );
        });
        let news_item5_mock = server.mock(|when, then| {
            when.method(GET).path("/item/15.json");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "id": 15,
                        "time": 1736908019,
                        "title": "Item 15",
                        "url": "https://www.cnbc.com/2025/01/14/item15.html"
                    }"#,
                );
        });

        let config = crate::AppConfig {
            api_base_url: expected_addr_str,
            db_dsn: ":memory:".to_string(),
            email_to: None,
            filters: vec![ItemFilter {
                value: "item".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            purge_after_days: 7,
            blacklisted_domains: vec![],
        };

        let fetcher = crate::Fetcher::new(&config);
        let op = crate::FetchOperation::Fetch(true);
        let num_fetched = fetcher.run(&op).await.unwrap();

        prefetch_mock.assert();
        news_item_mock.assert();
        news_item5_mock.assert();

        assert_eq!(num_fetched, 1, "Fetched items count is wrong");
    }

    #[test]
    /// Test filtering items based on the filters; use simple and regex filters
    pub async fn test_filtering_items() {
        let pulled_items = vec![
            DigestItem {
                news_title: "Rust is awesome".to_string(),
                news_url: "https://example.com".to_string(),
                created_at: 1700000000,
                id: 1,
            },
            DigestItem {
                news_title: "Rust is cool".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 2,
            },
            DigestItem {
                news_title: "Rust is aweful".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 3,
            },
            DigestItem {
                news_title: "Go is cool".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 4,
            },
            DigestItem {
                news_title: "Dart is some thing".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 5,
            },
        ];
        let mut config = crate::AppConfig {
            api_base_url: "https://localhost/v0".to_string(),
            db_dsn: ":memory:".to_string(),
            email_to: None,
            filters: vec![
                ItemFilter {
                    value: "cool".to_string(),
                    title: "PLs".to_string(),
                },
                ItemFilter {
                    value: "awesome".to_string(),
                    title: "PLs".to_string(),
                },
            ],
            smtp: None,
            purge_after_days: 7,
            blacklisted_domains: vec![],
        };
        let fetcher = crate::Fetcher::new(&config);

        assert_eq!(
            pulled_items
                .iter()
                .filter(|i| fetcher.keep_item(&i.news_title, false))
                .count(),
            3,
            "Filtering items agains mutiple simple filters failed",
        );

        config.filters = vec![ItemFilter {
            value: "some\\b".to_string(),
            title: "PLs".to_string(),
        }];

        let fetcher = crate::Fetcher::new(&config);
        assert_eq!(
            pulled_items
                .iter()
                .filter(|i| fetcher.keep_item(&i.news_title, false))
                .count(),
            2,
            "Filtering items against a regex filter failed",
        );
    }
}
