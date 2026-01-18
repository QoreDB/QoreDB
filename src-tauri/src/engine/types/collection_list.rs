use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionListOptions {
    pub search: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

impl Default for CollectionListOptions {
    fn default() -> Self {
        Self {
            search: None,
            page: None,
            page_size: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionList {
    pub collections: Vec<Collection>,
    pub total_count: u32,
}
