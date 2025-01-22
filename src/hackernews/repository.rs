use crate::schemas::prelude::news_items;
use diesel::prelude::*;
use dotenvy::dotenv;

#[derive(diesel::MultiConnection)]
pub enum AnyConnection {
    Mysql(diesel::MysqlConnection),
    Sqlite(diesel::SqliteConnection),
}

/// DB Model: A news item that has been fetched
#[derive(Debug, Clone, Queryable, Selectable, PartialEq, Insertable)]
#[diesel(table_name = news_items)]
pub struct DigestItem {
    pub id: i32,
    pub news_title: String,
    pub news_url: String,
    pub created_at: i32,
}

#[derive(Debug, Clone, PartialEq, Selectable)]
#[diesel(table_name = news_items)]
pub struct DigestItemID {
    pub id: i32,
}

pub fn establish_connection(database_url: &String) -> AnyConnection {
    dotenv().ok();

    if database_url.starts_with("mysql") {
        AnyConnection::Mysql(establish_mysql_conn(database_url))
    } else {
        AnyConnection::Sqlite(establish_sqlite_conn(database_url))
    }
}

pub fn establish_sqlite_conn(database_url: &String) -> SqliteConnection {
    dotenv().ok();

    SqliteConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

pub fn establish_mysql_conn(database_url: &String) -> MysqlConnection {
    dotenv().ok();

    MysqlConnection::establish(&database_url)
        .unwrap_or_else(|s| panic!("Error connecting to {} with {}", database_url, s))
}

/// Vacuum the database - remove news items which created_at is older than `expire_after_days`
pub fn vacuum(
    expire_after_days: i32,
    conn: &mut AnyConnection,
) -> Result<usize, diesel::result::Error> {
    use crate::schemas::prelude::news_items::dsl::*;

    let now = chrono::Utc::now().timestamp();
    let expire_after = now - (expire_after_days as i64 * 24 * 60 * 60);
    let num_deleted =
        diesel::delete(news_items.filter(created_at.lt(expire_after as i32))).execute(conn)?;

    Ok(num_deleted)
}

/// Get IDs of the news items whose IDs are not in the database yet
pub fn get_ids_to_pull(prefetched_ids: Vec<i32>, conn: &mut AnyConnection) -> Vec<i32> {
    use crate::schemas::prelude::news_items::dsl::{id, news_items};

    let existing_ids: Vec<i32> = news_items
        .select(id)
        .filter(id.eq_any(&prefetched_ids))
        .load::<i32>(conn)
        .expect("Error loading IDs");

    let ids_to_pull: Vec<i32> = prefetched_ids
        .into_iter()
        .filter(|item_id| !existing_ids.contains(&(*item_id as i32)))
        .collect();

    ids_to_pull
}

/// Store the news items in the database
pub fn store_news_items(
    digest: &Vec<DigestItem>,
    conn: &mut AnyConnection,
) -> Result<(), diesel::result::Error> {
    use crate::schemas::prelude::news_items::dsl::*;

    if let AnyConnection::Mysql(ref mut conn) = conn {
        diesel::insert_into(news_items)
            .values(digest)
            .execute(conn)?;
    } else if let AnyConnection::Sqlite(ref mut conn) = conn {
        diesel::insert_into(news_items)
            .values(digest)
            .execute(conn)?;
    } else {
        return Err(diesel::result::Error::QueryBuilderError(
            "Unsupported connection type".into(),
        ));
    }

    Ok(())
}
