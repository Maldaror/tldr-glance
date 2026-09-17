use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct RawStory {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub canonical_domain: Option<String>,
    #[serde(default)]
    pub estimated_reading_minutes: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Story {
    pub title: String,
    pub url: String,
    pub summary: String,
    pub category: String,
    pub domain: String,
    pub reading_minutes: u32,
}

impl Story {
    pub fn from_raw(raw: RawStory) -> Self {
        Story {
            title: raw.title,
            url: raw.url,
            summary: raw
                .summary
                .or(raw.topic)
                .unwrap_or_else(|| "(keine Zusammenfassung)".to_string()),
            category: raw.category.unwrap_or_else(|| "misc".to_string()),
            domain: raw.canonical_domain.unwrap_or_default(),
            reading_minutes: raw.estimated_reading_minutes.unwrap_or(0),
        }
    }
}
