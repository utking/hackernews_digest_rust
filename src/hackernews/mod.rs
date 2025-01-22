mod arg_parse;
mod config;
mod data_types;
mod fetcher;
mod filter;
mod repository;
mod sender;

pub mod prelude {
    pub const API_BASE_URL: &str = "https://hacker-news.firebaseio.com/v0";
    pub use super::super::schemas::prelude::*;
    pub use super::arg_parse::*;
    pub use super::config::*;
    pub use super::data_types::*;
    pub use super::fetcher::*;
    pub use super::filter::*;
    pub use super::repository::*;
    pub use super::sender::*;
    pub use regex::{Regex, RegexBuilder};
    pub use serde::{Deserialize, Serialize};
    pub use url::Url;
}
