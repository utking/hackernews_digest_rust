use crate::{
    establish_connection, schemas::prelude::run_migrations, AppConfig, Digest, DigestItem,
    DigestSender, DummySender, JsonNewsItem, SenderType, SmtpSender, TelegramSender, API_BASE_URL,
};
use diesel::SqliteConnection;
use regex::{Regex, RegexBuilder};
use url::Url;

#[derive(Debug, Clone)]
pub enum FetchOperation {
    Fetch(bool),
    Vacuum,
}

pub struct Fetcher {
    pub config: AppConfig,
    api_base_url: String,
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
            api_base_url: API_BASE_URL.to_string(),
        }
    }

    #[allow(dead_code)]
    fn with_base_url(&self, base_url: String) -> Self {
        Self {
            config: self.config.clone(),
            filters: self.filters.clone(),
            api_base_url: base_url,
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
                    match self.config.get_sender() {
                        SenderType::Email(config) => {
                            SmtpSender::new(&config).send_digest(&digest).await?
                        }
                        SenderType::Telegram(config) => {
                            TelegramSender::new(&config).send_digest(&digest).await?
                        }
                        SenderType::Console => DummySender {}.send_digest(&digest).await?,
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
        let mut skipped: Digest = Vec::new();

        let prefetched = self.prefetch().await?;
        let ids_to_pull = crate::get_ids_to_pull(prefetched, conn);

        for id in ids_to_pull {
            let news_item = &self.fetch_news_item(id).await?;
            let digest_item = &news_item.as_digest_item();

            // Skip blacklisted domains, but store the news item in the database
            if self.is_blacklisted(&digest_item.news_url) {
                skipped.push(digest_item.clone());
                continue;
            }

            // Skip items with missing URLs from the digest, but store them in the database
            if self.is_missing_url(&digest_item.news_url) {
                skipped.push(digest_item.clone());
                continue;
            }

            // Apply filters
            if !self.keep_item(&digest_item.news_title.clone(), reverse) {
                skipped.push(digest_item.clone());
                continue;
            }

            digest.push(digest_item.clone());
        }

        // Store the skipped news items in the database
        crate::store_news_items(&skipped, &mut conn)?;
        // Store the news items in the database
        crate::store_news_items(&digest, &mut conn)?;

        Ok(self.deduplicate(&digest))
    }

    /// Keep an item based on the filters. If reverse is true, keep the item if it doesn't match
    fn keep_item(&self, title: &String, reverse: bool) -> bool {
        let mut keep: bool = false;
        if reverse {
            for filter in &self.filters {
                if !filter.is_match(&title) {
                    keep = true;
                    break;
                }
            }
        } else {
            for filter in &self.filters {
                if filter.is_match(&title) {
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
        let result = reqwest::get(format!("{}/topstories.json", self.api_base_url))
            .await?
            .json::<Vec<i32>>()
            .await?;

        Ok(result)
    }

    /// Fetch a single news item by its ID
    async fn fetch_news_item(&self, id: i32) -> Result<JsonNewsItem, Box<dyn std::error::Error>> {
        let result = reqwest::get(format!("{}/item/{id}.json", self.api_base_url))
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

    /// De-duplicate the fetched items and return the unique items. URL is used as the key.
    fn deduplicate(&self, items: &Vec<DigestItem>) -> Vec<DigestItem> {
        let mut unique_items: Vec<DigestItem> = Vec::new();
        let mut urls: Vec<String> = Vec::new();

        for item in items {
            if !urls.contains(&item.news_url.clone()) {
                urls.push(item.news_url.clone());
                unique_items.push(item.clone());
            }
        }

        unique_items
    }
}

#[cfg(test)]
mod test {
    use crate::{schemas::prelude::run_migrations, AppConfig, DigestItem, ItemFilter};
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
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
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
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
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
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let fetcher = crate::Fetcher::new(&config).with_base_url(expected_addr_str);

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
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };

        let fetcher = crate::Fetcher::new(&config).with_base_url(expected_addr_str);
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
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let fetcher = crate::Fetcher::new(&config).with_base_url(expected_addr_str);

        let mut conn = crate::establish_connection(&config.db_dsn);
        // apply migrations
        run_migrations(&mut conn).unwrap();
        // store the pulled items in the database to have IDs to pull
        crate::store_news_items(&pulled_items, &mut conn).unwrap();

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
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: ".*".to_string(), // match all
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };

        let fetcher = crate::Fetcher::new(&config).with_base_url(expected_addr_str);
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
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "item".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            purge_after_days: 7,
            blacklisted_domains: vec![],
        };

        let fetcher = crate::Fetcher::new(&config).with_base_url(expected_addr_str);
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
            db_dsn: ":memory:".to_string(),
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
            telegram: None,
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

    #[test]
    /// Test filtering items based on the filters; use simple and regex filters
    pub async fn test_filter_config_from_str() {
        let pulled_items = vec![
            DigestItem {
                news_title: "So You Want to Build Your Own Data Center".to_string(),
                news_url: "https://example.com".to_string(),
                created_at: 1700000000,
                id: 1,
            },
            DigestItem {
                news_title: "Maze Generation: Recursive Division (2011)".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 2,
            },
            DigestItem {
                news_title: "Swedish Exports of Ball Bearings".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 3,
            },
            DigestItem {
                news_title: "Obelisks".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 4,
            },
            DigestItem {
                news_title: "Bluesky accounts add 10k followers per day".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 5,
            },
        ];
        let config = AppConfig::from_str(
            r#"{
                "purge_after_days": 720,
                "db_dsn": "hackernews_db.sqlite",
                "blacklisted_domains": [
                    "www.businessinsider.com",
                    "www.nytimes.com",
                    "en.wikipedia.org",
                    "twitter.com",
                    "www.wsj.com",
                    "www.cnbc.com",
                    "seekingalpha.com",
                    "www.washingtonpost.com",
                    "www.bloomberg.com",
                    "www.ft.com",
                    "fortune.com"
                ],
                "filters": [
                    {"title": "IDE", "value": "\\bzed\\b,\\b(vs|studio)\\s?code\\b,\\bvim\\b,neovim,\\bide\\b"},
                    {"title": "JavaScript", "value": "\\bjs\\b,(ecma|java).*script,\\bnode(\\.?js)?\\b,\\bnpm\\b"},
                    {"title": "Covid", "value": "\\bcovid,\\bdelta\\b,vaccin"},
                    {"title": "SQL", "value": "sql,database"},
                    {"title": "Languages", "value": "\\bgo(lang)?\\b,\\brust\\b,\\bphp\\b,\\bmarkdown\\b,crystal,carbon\\b,pattern"},
                    {"title": "FreeStuff", "value": "\\bfree\\b"},
                    {"title": "HardwareVendors", "value": "dell"},
                    {"title": "GraphQL", "value": "graphql"},
                    {"title": "API", "value": "api\\b"},
                    {"title": "Misc", "value": "toolbox\\b,framework\\b,\\bsdk\\b,\\bui\\b,\\barchitect"},
                    {"title": "Hackers", "value": "\\bcve-,\\bhack,\\bpassw,\\bsecuri,\\bvulner,\\bbot\\b,\\bbotnet,owasp"},
                    {"title": "Development", "value": "development,\\bweb.?socket,gdb"},
                    {"title": "Css", "value": "\\bcss\\b,\\bstyle\\b"},
                    {"title": "Linux", "value": "\\blinux\\b,ubuntu,debian,centos,\\bgnu\\b,\\bopen[-\\s]?source\\b,bpf\\b,tcp,ssh"},
                    {
                        "title": "Services",
                        "value": "docker,haproxy,cassandra,elasticsearch,rabbitmq,nginx,k8s,\\brke,\\brancher,kubernetes,postfix,https,apache,github,\\bgit\\b"
                    },
                    {
                        "title": "FAANG",
                        "value": "google,apple,\\bmeta\\b,facebook,\\bfb\\b,microsoft,\\bms\\b,netflix,whatsapp,amazon,\\baws\\b"
                    },
                    {"title": "SRE", "value": "\\bsre\\b,devops,resiliency,recovery,reliability"},
                    {"title": "Vue", "value": "\\bvue(\\.?js)?\\b"},
                    {"title": "Books", "value": "pdf"},
                    {"title": "Primers", "value": "primer\\b"},
                    {"title": "Awesome", "value": "awesome\\b"},
                    {"title": "AppNews", "value": "\\bapp\\b"},
                    {"title": "Python", "value": "\\bpython"},
                    {"title": "Problem", "value": "version,problem,debug,issues?\\b"},
                    {"title": "Releases", "value": "release,\\bannounc"},
                    {"title": "CPU/GPU", "value": "\\bintel\\b,\\bamd\\b"},
                    {"title": "ComputerScience", "value": "\\bcs-?[1-9],\\balgor"},
                    {"title": "Illinois", "value": "chicago,illinois"},
                    {"title": "Deals and Discounts", "value": "black\\s*friday,\\bdeals?\\b,discount,coupon"}
                ],
                "email_to": ""
            }
            "#).unwrap();
        let fetcher = crate::Fetcher::new(&config);

        assert_eq!(fetcher.config.filters.len(), 29, "Filters count is wrong");
        assert_eq!(fetcher.filters.len(), 105, "Parsed filters count is wrong");

        assert_eq!(
            pulled_items
                .iter()
                .filter(|i| fetcher.keep_item(&i.news_title, false))
                .count(),
            0,
            "Filtering items agains mutiple simple filters failed",
        );
    }

    /// Test the deduplication of the fetched items
    #[test]
    pub async fn test_deduplication() {
        let pulled_items = vec![
            DigestItem {
                news_title: "Item #1".to_string(),
                news_url: "https://example.com".to_string(),
                created_at: 1700000000,
                id: 1,
            },
            DigestItem {
                news_title: "Item #2".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 2,
            },
            DigestItem {
                news_title: "Some other name for item #1".to_string(),
                news_url: "https://example.com".to_string(),
                created_at: 1700000000,
                id: 3,
            },
            DigestItem {
                news_title: "Item #2 duplicate".to_string(),
                news_url: "https://example.org".to_string(),
                created_at: 1700000000,
                id: 4,
            },
        ];
        let config = crate::AppConfig {
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            purge_after_days: 7,
            blacklisted_domains: vec![],
        };
        let fetcher = crate::Fetcher::new(&config);

        let deduplicated = fetcher.deduplicate(&pulled_items);
        assert_eq!(deduplicated.len(), 2, "Deduplication failed");

        assert_eq!(deduplicated[0].id, 1, "Deduplication failed");
        assert_eq!(deduplicated[1].id, 2, "Deduplication failed");
        assert_eq!(
            deduplicated[0].news_title, "Item #1",
            "Deduplication failed"
        );
        assert_eq!(
            deduplicated[1].news_title, "Item #2",
            "Deduplication failed"
        );
    }
}
