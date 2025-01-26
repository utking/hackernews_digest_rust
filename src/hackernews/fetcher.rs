use crate::{
    common::{deduplicate, is_missing_url},
    config,
    storage::{FileStorage, Record, Storage},
    DigestItem, Fetch, Filters, JsonNewsItem, Regex, Url,
};
use config::AppConfig;

pub struct HNFetcher {
    pub config: AppConfig,
    storage: Storage,
    api_base_url: String,
    filters: Vec<Regex>,
}

impl HNFetcher {
    #[must_use]
    /// Create a new fetcher with the given configuration
    pub fn new(config: &AppConfig, storage: Storage) -> HNFetcher {
        const API_BASE_URL: &str = "https://hacker-news.firebaseio.com/v0";
        Self {
            config: config.clone(),
            filters: Filters::compile(&config.filters),
            api_base_url: API_BASE_URL.to_string(),
            storage,
        }
    }

    #[allow(dead_code)]
    fn with_base_url(&mut self, base_url: String) -> &mut Self {
        self.api_base_url = base_url;
        self
    }

    /// Fetch not previously fetched news items from the API. For that, we need to:
    /// 1. Fetch the top stories' IDs from the API
    /// 2. Fetch each news item by its ID if it wasn't previously fetched; existing
    ///    news items' IDs are stored in the database
    /// 3. Apply filters to each news item
    /// 4. Store the news items in the database
    /// 5. Return the digest of new items fetched
    async fn fetch(
        &mut self,
        reverse: bool,
    ) -> Result<Vec<DigestItem>, Box<dyn std::error::Error>> {
        let mut digest = Vec::new();
        let mut skipped = Vec::new();

        let prefetched = self.prefetch().await?;
        let ids_to_pull = self.get_ids_to_pull("hackernews", prefetched);

        for id in ids_to_pull {
            let news_item = self.fetch_news_item(id).await?;
            let digest_item: DigestItem = news_item.into();

            // Skip blacklisted domains, but store the news item in the database
            if self.is_blacklisted(&digest_item.news_url) {
                skipped.push(DigestItem {
                    news_title: String::from("-"),
                    news_url: String::from("-"),
                    created_at: digest_item.created_at,
                    id: digest_item.id,
                });
                continue;
            }

            // Skip items with missing URLs from the digest, but store them in the database
            if is_missing_url(&digest_item.news_url) {
                skipped.push(DigestItem {
                    news_title: String::from("-"),
                    news_url: String::from("-"),
                    created_at: digest_item.created_at,
                    id: digest_item.id,
                });
                continue;
            }

            // Apply filters
            if !self.keep_item(&digest_item.news_title.clone(), reverse) {
                skipped.push(DigestItem {
                    news_title: String::from("-"),
                    news_url: String::from("-"),
                    created_at: digest_item.created_at,
                    id: digest_item.id,
                });
                continue;
            }

            digest.push(DigestItem {
                news_title: digest_item.news_title.clone(),
                news_url: digest_item.news_url.clone(),
                created_at: digest_item.created_at,
                id: digest_item.id,
            });
        }

        // Store the skipped news items in the database
        self.store_items("", &skipped)?;
        // Store the news items in the database
        self.store_items("", &digest)?;

        Ok(deduplicate(&digest))
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

    /// Fetch the top stories' IDs from the API
    async fn prefetch(&self) -> Result<Vec<i64>, Box<dyn std::error::Error>> {
        let result = reqwest::get(format!("{}/topstories.json", self.api_base_url))
            .await?
            .json::<Vec<i64>>()
            .await?;

        Ok(result)
    }

    /// Fetch a single news item by its ID
    async fn fetch_news_item(&self, id: i64) -> Result<JsonNewsItem, Box<dyn std::error::Error>> {
        let result = reqwest::get(format!("{}/item/{id}.json", self.api_base_url))
            .await?
            .json::<JsonNewsItem>()
            .await?;

        Ok(result)
    }

    /// Check if a URL's domain is in the blacklist
    fn is_blacklisted(&self, url: &str) -> bool {
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

impl Fetch for HNFetcher {
    /// Run the fetcher with the given operation. The operation can be either fetching
    /// new news items or vacuuming the database. Return the number of items fetched.
    /// If digest is not empty, send an email with the digest to the email address in the config.
    async fn run(&mut self, reverse: bool) -> Result<usize, Box<dyn std::error::Error>> {
        let digest = self.fetch(reverse).await?;
        // Send an email with the digest if it's not empty
        if !digest.is_empty() {
            // send the digest to the email address in the config, if given
            self.config
                .get_sender()
                .send_digest("HackerNews", &digest)
                .await?;
        }
        Ok(digest.len())
    }

    fn store_items(
        &mut self,
        _source: &str,
        items: &[DigestItem],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let records: Vec<Record> = items
            .iter()
            .map(|item| Record {
                id: item.id,
                source: "hackernews".to_string(),
                created_at: item.created_at,
            })
            .collect();
        match self.storage.insert_items(&records) {
            Ok(()) => {
                self.storage.dump()?;
                Ok(())
            }
            Err(e) => Err(Box::new(e)),
        }
    }

    fn get_ids_to_pull(&self, source_name: &str, prefetched_ids: Vec<i64>) -> Vec<i64> {
        let existing_ids = self.storage.query_ids(source_name);
        let ids_to_pull: Vec<i64> = prefetched_ids
            .into_iter()
            .filter(|item_id| !existing_ids.contains(item_id))
            .collect();

        ids_to_pull
    }
}

#[cfg(test)]
mod test {
    use super::{config::AppConfig, Fetch, HNFetcher};
    use crate::{
        common::{deduplicate, is_missing_url},
        storage::{self, FileStorage},
        DigestItem, Filters, ItemFilter,
    };
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

        assert_eq!(
            pulled_items
                .iter()
                .filter(|i| is_missing_url(&i.news_url))
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
        let config = AppConfig {
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            rss_sources: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let storage = storage::Storage::from_fs(tempfile::tempfile().unwrap());
        let fetcher = crate::HNFetcher::new(&config, storage);

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

        let config = AppConfig {
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            rss_sources: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let storage = storage::Storage::from_fs(tempfile::tempfile().unwrap());
        let mut fetcher = crate::HNFetcher::new(&config, storage);
        let fetcher = fetcher.with_base_url(expected_addr_str);

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

        let config = AppConfig {
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            rss_sources: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };

        let mut fetcher = crate::HNFetcher::new(
            &config,
            storage::Storage::from_fs(tempfile::tempfile().unwrap()),
        );
        let fetcher = fetcher.with_base_url(expected_addr_str);
        let item = fetcher.fetch_news_item(111).await.unwrap();
        prefetch_mock.assert();
        let digest_item: DigestItem = item.into();
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
        let config = AppConfig {
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "rust".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            rss_sources: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };
        let mut fetcher = crate::HNFetcher::new(
            &config,
            storage::Storage::from_fs(tempfile::tempfile().unwrap()),
        );
        let fetcher = &mut fetcher.with_base_url(expected_addr_str);

        // store the pulled items in the database to have IDs to pull
        // crate::store_news_items(&pulled_items, &mut storage).unwrap();
        fetcher.store_items("hackernews", &pulled_items).unwrap();

        let prefetched = fetcher.prefetch().await.unwrap();
        prefetch_mock.assert();

        let ids_to_pull = fetcher.get_ids_to_pull("hackernews", prefetched);
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

        let config = AppConfig {
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: ".*".to_string(), // match all
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            rss_sources: None,
            purge_after_days: 7,
            blacklisted_domains: vec![String::from("example.com")],
        };

        let mut fetcher = HNFetcher::new(
            &config,
            storage::Storage::from_fs(tempfile::tempfile().unwrap()),
        );
        let fetcher = fetcher.with_base_url(expected_addr_str);
        let num_fetched = fetcher.run(false).await.unwrap();

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

        let config = AppConfig {
            db_dsn: ":memory:".to_string(),
            filters: vec![ItemFilter {
                value: "item".to_string(),
                title: "PLs".to_string(),
            }],
            smtp: None,
            telegram: None,
            rss_sources: None,
            purge_after_days: 7,
            blacklisted_domains: vec![],
        };

        let mut fetcher = crate::HNFetcher::new(
            &config,
            storage::Storage::from_fs(tempfile::tempfile().unwrap()),
        );
        let fetcher = fetcher.with_base_url(expected_addr_str);
        let num_fetched = fetcher.run(true).await.unwrap();

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
        let mut config = AppConfig {
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
            rss_sources: None,
            purge_after_days: 7,
            blacklisted_domains: vec![],
        };
        let mut fetcher = crate::HNFetcher::new(
            &config,
            storage::Storage::from_fs(tempfile::tempfile().unwrap()),
        );

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
        fetcher.filters = Filters::compile(&config.filters);

        let filtered = pulled_items.iter().filter(|i| {
            let keep = fetcher.keep_item(&i.news_title, false);
            dbg!(&i.news_title, keep);
            keep
        });

        assert_eq!(
            filtered.count(),
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

        let fetcher = crate::HNFetcher::new(
            &config,
            storage::Storage::from_fs(tempfile::tempfile().unwrap()),
        );

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

        let deduplicated = deduplicate(&pulled_items);
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
