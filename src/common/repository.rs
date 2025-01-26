use crate::storage::{FileStorage, Storage};
use std::io::Error;

#[derive(Debug, Clone)]
/// DB Model: A news item that has been fetched
pub struct DigestItem {
    pub id: i64,
    pub news_title: String,
    pub news_url: String,
    pub created_at: i64,
}

/// Vacuum the database to remove old items
pub fn vacuum(storage: &mut Storage, purge_after_days: i64) -> Result<usize, Error> {
    let num_deleted = storage.vacuum(purge_after_days)?;
    storage.dump()?;

    Ok(num_deleted)
}
