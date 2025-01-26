mod data_types;
mod fetcher;

pub mod prelude {
    pub use super::super::common::prelude::*;
    pub use super::data_types::*;
    pub use super::fetcher::*;
    pub use regex::{Regex, RegexBuilder};
    pub use serde::{Deserialize, Serialize};
    pub use url::Url;
}
