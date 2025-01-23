use crate::*;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ItemFilter {
    pub title: String,
    pub value: String,
}

pub struct Filters {}

impl Filters {
    #[must_use]
    pub fn compile(filters: Vec<ItemFilter>) -> Vec<Regex> {
        let string_filters: Vec<String> = filters
            .iter()
            .flat_map(|f| f.value.split(',').map(|s| s.to_string()))
            .collect();

        let mut filters: Vec<Regex> = Vec::new();
        for filter in string_filters {
            match RegexBuilder::new(&filter.to_lowercase())
                .case_insensitive(true)
                .build()
            {
                Ok(re) => filters.push(re),
                Err(e) => eprintln!("Error creating filter: {e}"),
            }
        }
        filters
    }
}
