use crate::{Deserialize, Regex, RegexBuilder};

#[derive(Clone, Deserialize)]
pub struct ItemFilter {
    // pub title: String,
    pub value: String,
}

pub struct Filters {}

impl Filters {
    #[must_use]
    pub fn compile(filters: &[ItemFilter]) -> Vec<Regex> {
        let string_filters: Vec<String> = filters
            .iter()
            .flat_map(|f| f.value.split(',').map(std::string::ToString::to_string))
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
