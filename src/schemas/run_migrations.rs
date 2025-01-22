use crate::AnyConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub fn run_migrations(
    connection: &mut AnyConnection,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    if let AnyConnection::Mysql(ref mut conn) = connection {
        pub const MYSQL_MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/mysql");
        conn.run_pending_migrations(MYSQL_MIGRATIONS)?;
    } else if let AnyConnection::Sqlite(ref mut conn) = connection {
        pub const SQLITE_MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/sqlite");
        conn.run_pending_migrations(SQLITE_MIGRATIONS)?;
    } else {
        return Err("Unsupported connection type".into());
    }

    Ok(())
}
