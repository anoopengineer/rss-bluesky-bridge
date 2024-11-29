use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct ItemIdentifier {
    pub execution_id: String,
    pub guid: String,
}

#[derive(Serialize, Deserialize)]
pub struct RssItem {
    pub guid: String,
    pub title: String,
    pub description: String,
    pub link: String,
    pub pub_date: String,
}
