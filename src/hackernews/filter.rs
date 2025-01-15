use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ItemFilter {
    pub title: String,
    pub value: String,
}
