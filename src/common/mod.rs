use crate::{feeds::prelude::RssFetcher, DigestItem, HNFetcher};

mod filter;
mod repository;

pub enum FetcherType {
    HNFetcher(HNFetcher),
    RssFetcher(RssFetcher),
}

pub trait Fetch {
    async fn run(&mut self, reverse: bool) -> Result<usize, Box<dyn std::error::Error>>;
    fn store_items(
        &mut self,
        source: &str,
        items: &[DigestItem],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn get_ids_to_pull(&self, source_name: &str, prefetched_ids: Vec<i64>) -> Vec<i64>;
}

/// De-duplicate the fetched items and return the unique items. URL is used as the key.
pub fn deduplicate(items: &Vec<DigestItem>) -> Vec<DigestItem> {
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

/// Check if a URL is missing or empty in the digest item
pub fn is_missing_url(item_url: &String) -> bool {
    item_url.is_empty() || item_url == "-"
}

pub mod prelude {
    pub use super::filter::*;
    pub use super::repository::*;
    pub use super::Fetch;
}
