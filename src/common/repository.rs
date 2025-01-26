use crate::storage::{FileStorage, Record, Storage};
use std::io::Error;

#[derive(Debug, Clone)]
/// DB Model: A news item that has been fetched
pub struct DigestItem {
    pub id: i64,
    pub news_title: String,
    pub news_url: String,
    pub created_at: i64,
}

/// Get IDs of the news items whose IDs are not in the database yet
pub fn get_ids_to_pull(
    source_name: &str,
    prefetched_ids: Vec<i64>,
    storage: &mut Storage,
) -> Vec<i64> {
    let existing_ids = storage.query_ids(source_name);
    let ids_to_pull: Vec<i64> = prefetched_ids
        .into_iter()
        .filter(|item_id| !existing_ids.contains(item_id))
        .collect();

    ids_to_pull
}

/// Store the news items in the database. It's the same feed generally,
/// so we just give it a source
pub fn store_news_items(digest: &[DigestItem], storage: &mut Storage) -> Result<(), Error> {
    let items: Vec<Record> = digest
        .iter()
        .map(|item| Record {
            id: item.id,
            source: "hackernews".to_string(),
            created_at: item.created_at,
        })
        .collect();
    storage.insert_items(&items)?;
    storage.dump()?;

    Ok(())
}

/// Store the RSS items in the database. The `feed_source` is part of the primary key
/// so we can store multiple feeds in the same table
pub fn store_feed_items(
    feed_source: &str,
    digest: &[DigestItem],
    storage: &mut Storage,
) -> Result<(), Error> {
    let current_timestamp = chrono::Utc::now().timestamp();

    let feed_items: Vec<Record> = digest
        .iter()
        .map(|item| Record {
            id: item.id,
            source: feed_source.to_string(),
            created_at: current_timestamp,
        })
        .collect();

    storage.insert_items(&feed_items)?;
    storage.dump()?;

    Ok(())
}

/// Vacuum the database to remove old items
pub fn vacuum(storage: &mut Storage, purge_after_days: usize) -> Result<usize, Error> {
    let num_deleted = storage.vacuum(purge_after_days)?;
    storage.dump()?;

    Ok(num_deleted)
}
