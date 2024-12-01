use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ItemIdentifier {
    pub execution_id: String,
    pub guid: String,
}

/// Represents an execution item in the DynamoDB table.
///
/// This struct contains all the fields associated with an execution item,
/// including metadata and content-related information. An execution item is one item that we got from the RSS feed that we want to process further via other lambdas, such as summarize the description and post to bluesky. We temporarily store this in ddb so that, we don't have to pass huge data as input/output in state functions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionItem {
    /// Each step function execution has a unique execution id. This becomes the PK of the record
    pub execution_id: String,
    /// Globally unique identifier for the rss item that we get from the RSS feed
    pub guid: String,
    /// Title of the RSS item.
    pub title: Option<String>,
    /// Description of the RSS item.
    pub description: Option<String>,
    /// URL link associated with the RSS item.
    pub link: Option<String>,
    /// summary field that we populate via AI summarization. Not obtained from RSS.
    pub summary: Option<String>,
    /// Time-to-live value for DynamoDB, in Unix timestamp format.
    pub ttl: Option<i64>,
    /// Type identifier for the item, always set to "ExecutionItem".
    pub _type: Option<String>,
    /// Publication date of the RSS item.
    pub pub_date: Option<String>,
}

impl ExecutionItem {
    /// Creates a new ExecutionItem with the given parameters.
    ///
    /// This method generates a new UUID for the item and sets the TTL to 24 hours from now.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - A string that holds the unique identifier for the execution.
    /// * `title` - A string that holds the title of the execution item.
    /// * `description` - A string that describes the execution item.
    /// * `link` - A string that contains the URL associated with the execution item.
    /// * `pub_date` - A string that represents the publication date of the execution item.
    ///
    /// # Returns
    ///
    /// A new instance of ExecutionItem.
    pub fn new(
        execution_id: String,
        guid: String,
        title: Option<String>,
        description: Option<String>,
        link: Option<String>,
        pub_date: Option<String>,
    ) -> Result<Self> {
        if execution_id.trim().is_empty() || guid.trim().is_empty() {
            Err(anyhow!("execution_id and guid cannot be empty"))
        } else {
            let ttl = Utc::now() + chrono::Duration::hours(24);
            Ok(Self {
                execution_id,
                guid,
                title,
                description,
                link,
                summary: None,
                ttl: Some(ttl.timestamp()),
                _type: Some("ExecutionItem".to_string()),
                pub_date,
            })
        }
    }
}

/// Represents a RSS item that we have already published to bluesky stored in the DynamoDB table for deduping.
///
/// This struct contains just the guid field necessary to identify whether an RSS item has already been published or not. We specifically don't want to include all metadata here to save cost. This has an infinite TTL compared to the extremely short TTL of ExecutionItem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordItem {
    /// Globally unique identifier for the record item.
    pub guid: String,
    /// Type identifier for the item, always set to "RecordItem".
    pub _type: Option<String>,
}

impl RecordItem {
    /// Creates a new RecordItem.
    ///
    /// This method generates a new UUID for the item and sets the _type to "RecordItem".
    ///
    /// # Returns
    ///
    /// A new instance of RecordItem.
    pub fn new(guid: String) -> Result<Self> {
        if guid.trim().is_empty() {
            Err(anyhow!("GUID cannot be empty"))
        } else {
            Ok(Self {
                guid,
                _type: Some("RecordItem".to_string()),
            })
        }
    }
}
