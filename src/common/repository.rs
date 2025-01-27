use crate::rss_items;
use diesel::prelude::*;

#[derive(Clone)]
/// DB Model: A news item that has been fetched
pub struct DigestItem {
    pub id: i64,
    pub news_title: String,
    pub news_url: String,
    pub created_at: i64,
}

#[derive(Clone, Queryable, Selectable, PartialEq, Insertable)]
#[diesel(table_name = rss_items)]
struct FeedItem {
    pub id: i64,
    pub source: String,
    pub created_at: i64,
}

#[derive(Clone, PartialEq, Selectable)]
#[diesel(table_name = rss_items)]
pub struct DigestItemID {
    pub id: i64,
}

pub fn establish_connection(database_url: &str) -> SqliteConnection {
    SqliteConnection::establish(database_url)
        .unwrap_or_else(|e| panic!("Error connecting to {database_url} with {e}"))
}

/// Vacuum the database - remove news items which `created_at` is older than `expire_after_days`
pub fn vacuum(
    expire_after_days: i64,
    conn: &mut SqliteConnection,
) -> Result<usize, diesel::result::Error> {
    use crate::schemas::prelude::rss_items::dsl::*;

    let expire_after = chrono::Utc::now().timestamp() - expire_after_days * 24 * 60 * 60;
    let num_deleted =
        diesel::delete(rss_items.filter(created_at.lt(expire_after))).execute(conn)?;
    Ok(num_deleted)
}

/// Get IDs of the news items whose IDs are not in the database yet
pub fn get_ids_to_pull(
    news_source: &str,
    prefetched_ids: Vec<i64>,
    conn: &mut SqliteConnection,
) -> Vec<i64> {
    use crate::schemas::prelude::rss_items::dsl::{id, rss_items, source};

    let existing_ids: Vec<i64> = rss_items
        .select(id)
        .filter(source.eq(news_source))
        .filter(id.eq_any(&prefetched_ids))
        .load::<i64>(conn)
        .expect("Error loading IDs");

    prefetched_ids
        .into_iter()
        .filter(|item_id| !existing_ids.contains(item_id))
        .collect()
}

/// Store the news items in the database. It's the same feed generally,
/// so we just give it a source
pub fn store_news_items(
    digest: &[DigestItem],
    conn: &mut SqliteConnection,
) -> Result<(), diesel::result::Error> {
    store_feed_items("hackernews", digest, conn)
}

/// Store the RSS items in the database. The `feed_source` is part of the primary key
/// so we can store multiple feeds in the same table
pub fn store_feed_items(
    feed_source: &str,
    digest: &[DigestItem],
    conn: &mut SqliteConnection,
) -> Result<(), diesel::result::Error> {
    use crate::schemas::prelude::rss_items::dsl::*;

    let current_timestamp = chrono::Utc::now().timestamp();
    let feed_items: Vec<FeedItem> = digest
        .iter()
        .map(|item| FeedItem {
            id: item.id,
            source: feed_source.to_string(),
            created_at: current_timestamp,
        })
        .collect();

    diesel::insert_into(rss_items)
        .values(feed_items)
        .execute(conn)?;

    Ok(())
}
