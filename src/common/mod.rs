mod filter;
mod repository;

#[derive(Debug, Clone)]
pub enum FetchOperation {
    Fetch(bool),
    Vacuum,
}

pub trait Fetch {
    async fn run(&self, op: &FetchOperation) -> Result<i32, Box<dyn std::error::Error>>;
}

pub mod prelude {
    pub use super::filter::*;
    pub use super::repository::*;
    pub use super::Fetch;
}
