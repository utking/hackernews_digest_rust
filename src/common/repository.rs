use diesel::prelude::*;
use dotenvy::dotenv;

use crate::rss_items;

#[derive(diesel::MultiConnection)]
pub enum AnyConnection {
    Mysql(diesel::MysqlConnection),
    Sqlite(diesel::SqliteConnection),
}

#[derive(Debug, Clone)]
/// DB Model: A news item that has been fetched
pub struct DigestItem {
    pub id: i32,
    pub news_title: String,
    pub news_url: String,
    pub created_at: i32,
}

#[derive(Debug, Clone, Queryable, Selectable, PartialEq, Insertable)]
#[diesel(table_name = rss_items)]
struct FeedItem {
    pub id: i32,
    pub source: String,
    pub created_at: i32,
}

#[derive(Debug, Clone, PartialEq, Selectable)]
#[diesel(table_name = rss_items)]
pub struct DigestItemID {
    pub id: i32,
}

pub fn establish_connection(database_url: &str) -> AnyConnection {
    dotenv().ok();

    if database_url.starts_with("mysql") {
        AnyConnection::Mysql(establish_mysql_conn(database_url))
    } else {
        AnyConnection::Sqlite(establish_sqlite_conn(database_url))
    }
}

pub fn establish_sqlite_conn(database_url: &str) -> SqliteConnection {
    dotenv().ok();

    SqliteConnection::establish(database_url)
        .unwrap_or_else(|e| panic!("Error connecting to {database_url} with {e}"))
}

pub fn establish_mysql_conn(database_url: &str) -> MysqlConnection {
    dotenv().ok();

    MysqlConnection::establish(database_url)
        .unwrap_or_else(|e| panic!("Error connecting to {database_url} with {e}"))
}

/// Vacuum the database - remove news items which `created_at` is older than `expire_after_days`
pub fn vacuum(
    expire_after_days: usize,
    conn: &mut AnyConnection,
) -> Result<usize, diesel::result::Error> {
    use crate::schemas::prelude::rss_items::dsl::*;

    let now = chrono::Utc::now().timestamp();
    let expire_after = now - (expire_after_days as i64 * 24 * 60 * 60);
    let num_deleted =
        diesel::delete(rss_items.filter(created_at.lt(expire_after as i32))).execute(conn)?;

    Ok(num_deleted)
}

/// Get IDs of the news items whose IDs are not in the database yet
pub fn get_ids_to_pull(prefetched_ids: Vec<i32>, conn: &mut AnyConnection) -> Vec<i32> {
    use crate::schemas::prelude::rss_items::dsl::{id, rss_items};

    let existing_ids: Vec<i32> = rss_items
        .select(id)
        .filter(id.eq_any(&prefetched_ids))
        .load::<i32>(conn)
        .expect("Error loading IDs");

    let ids_to_pull: Vec<i32> = prefetched_ids
        .into_iter()
        .filter(|item_id| !existing_ids.contains(item_id))
        .collect();

    ids_to_pull
}

/// Store the news items in the database. It's the same feed generally,
/// so we just give it a source
pub fn store_news_items(
    digest: &[DigestItem],
    conn: &mut AnyConnection,
) -> Result<(), diesel::result::Error> {
    store_feed_items("hackernews", digest, conn)
}

/// Store the RSS items in the database. The `feed_source` is part of the primary key
/// so we can store multiple feeds in the same table
pub fn store_feed_items(
    feed_source: &str,
    digest: &[DigestItem],
    conn: &mut AnyConnection,
) -> Result<(), diesel::result::Error> {
    use crate::schemas::prelude::rss_items::dsl::*;

    let current_timestamp = chrono::Utc::now().timestamp();

    let feed_items: Vec<FeedItem> = digest
        .iter()
        .map(|item| FeedItem {
            id: item.id,
            source: feed_source.to_string(),
            created_at: current_timestamp as _,
        })
        .collect();

    if let AnyConnection::Mysql(ref mut conn) = conn {
        diesel::insert_into(rss_items)
            .values(feed_items)
            .execute(conn)?;
    } else if let AnyConnection::Sqlite(ref mut conn) = conn {
        diesel::insert_into(rss_items)
            .values(feed_items)
            .execute(conn)?;
    } else {
        return Err(diesel::result::Error::QueryBuilderError(
            "Unsupported connection type".into(),
        ));
    }

    Ok(())
}
