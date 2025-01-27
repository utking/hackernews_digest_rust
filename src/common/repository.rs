use crate::rss_items;
use diesel::prelude::*;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

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

pub struct Storage {
    conn: SqliteConnection,
}

impl Storage {
    pub fn new(conn: SqliteConnection) -> Self {
        let mut s = Storage { conn };
        s.run_migrations().expect("Error running migrations");

        s
    }

    pub fn establish_connection(database_url: &str) -> SqliteConnection {
        SqliteConnection::establish(database_url)
            .unwrap_or_else(|e| panic!("Error connecting to {database_url} with {e}"))
    }

    /// Vacuum the database - remove news items which `created_at` is older than `expire_after_days`
    pub fn vacuum(&mut self, expire_after_days: i64) -> Result<usize, diesel::result::Error> {
        use crate::schemas::prelude::rss_items::dsl::*;

        let expire_after = chrono::Utc::now().timestamp() - expire_after_days * 24 * 60 * 60;
        let num_deleted = diesel::delete(rss_items.filter(created_at.lt(expire_after)))
            .execute(&mut self.conn)?;
        Ok(num_deleted)
    }

    /// Get IDs of the news items whose IDs are not in the database yet
    pub fn get_ids_to_pull(&mut self, news_source: &str, prefetched_ids: Vec<i64>) -> Vec<i64> {
        use crate::schemas::prelude::rss_items::dsl::{id, rss_items, source};

        let existing_ids: Vec<i64> = rss_items
            .select(id)
            .filter(source.eq(news_source))
            .filter(id.eq_any(&prefetched_ids))
            .load::<i64>(&mut self.conn)
            .expect("Error loading IDs");

        prefetched_ids
            .into_iter()
            .filter(|item_id| !existing_ids.contains(item_id))
            .collect()
    }

    /// Store the news items in the database. It's the same feed generally,
    /// so we just give it a source
    pub fn store_news_items(&mut self, digest: &[DigestItem]) -> Result<(), diesel::result::Error> {
        self.store_feed_items("hackernews", digest)
    }

    /// Store the RSS items in the database. The `feed_source` is part of the primary key
    /// so we can store multiple feeds in the same table
    pub fn store_feed_items(
        &mut self,
        feed_source: &str,
        digest: &[DigestItem],
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
            .execute(&mut self.conn)?;

        Ok(())
    }

    fn run_migrations(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        pub const SQLITE_MIGRATIONS: EmbeddedMigrations = embed_migrations!();
        self.conn.run_pending_migrations(SQLITE_MIGRATIONS)?;

        Ok(())
    }
}
