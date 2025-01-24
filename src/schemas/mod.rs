mod run_migrations;
mod schema;

pub mod prelude {
    pub use super::run_migrations::run_migrations;
    pub use super::schema::rss_items;
}
