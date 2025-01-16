use crate::{
    digest_to_html, digest_to_text, establish_connection, schemas::prelude::run_migrations,
    store_news_item, AppConfig, Digest, JsonNewsItem, Sender,
};
use diesel::SqliteConnection;
use regex::Regex;
use url::Url;

pub enum FetchOperation {
    Fetch(bool),
    Vacuum,
}

pub struct Fetcher {
    pub config: AppConfig,
    filters: Vec<Regex>,
    // news_repo: NewsRepository,
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
            match Regex::new(&filter) {
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
        let migration_conn = &mut establish_connection(&self.config.db_dsn);
        match run_migrations(migration_conn) {
            Ok(_) => {}
            Err(e) => eprintln!("Error running migrations: {}", e),
        }

        match op {
            FetchOperation::Fetch(reverse) => {
                let digest = self.fetch(*reverse).await?;
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
            FetchOperation::Vacuum => {
                let mut conn = establish_connection(&self.config.db_dsn);
                self.vacuum(&mut conn).await
            }
        }
    }

    /// Fetch not previously fetched news items from the API. For that, we need to:
    /// 1. Fetch the top stories' IDs from the API
    /// 2. Fetch each news item by its ID if it wasn't previously fetched; existing
    ///   news items' IDs are stored in the database
    /// 3. Apply filters to each news item
    /// 4. Store the news items in the database
    /// 5. Return the digest of new items fetched
    async fn fetch(&self, reverse: bool) -> Result<Digest, Box<dyn std::error::Error>> {
        let mut digest: Digest = Vec::new();
        let mut conn = establish_connection(&self.config.db_dsn);

        let prefetched = self.prefetch().await?;
        let ids_to_pull = crate::get_ids_to_pull(prefetched, &mut conn);

        for id in ids_to_pull[0..ids_to_pull.len()].iter() {
            let news_item = &self.fetch_news_item(*id).await?;
            let digest_item = &news_item.as_digest_item();
            let title = &digest_item.news_title.clone();

            // Skip blacklisted domains, but still store the news item in the database
            if self.is_blacklisted(&digest_item.news_url) {
                store_news_item(digest_item, &mut conn)?;
                continue;
            }

            // Apply filters
            if reverse {
                for filter in &self.filters {
                    if !filter.is_match(&title) {
                        digest.push(digest_item.clone());
                        break;
                    }
                }
            } else {
                for filter in &self.filters {
                    if filter.is_match(&title) {
                        digest.push(digest_item.clone());
                        break;
                    }
                }
            }
        }

        // Store the news items in the database
        crate::store_digest(&digest, &mut conn)?;

        Ok(digest)
    }

    /// Vacuum the database - remove old news items
    async fn vacuum(&self, conn: &mut SqliteConnection) -> Result<i32, Box<dyn std::error::Error>> {
        let num_deleted = crate::vacuum(self.config.purge_after_days as i32, conn)?;

        Ok(num_deleted as i32)
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
