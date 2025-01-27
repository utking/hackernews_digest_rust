use diesel::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub fn run_migrations(
    conn: &mut SqliteConnection,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    pub const SQLITE_MIGRATIONS: EmbeddedMigrations = embed_migrations!();
    conn.run_pending_migrations(SQLITE_MIGRATIONS)?;

    Ok(())
}
