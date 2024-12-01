use crate::models::{ExecutionItem, RecordItem};
use anyhow::{Context, Result};
use aws_sdk_dynamodb::types::{AttributeValue, DeleteRequest, PutRequest, WriteRequest};
use aws_sdk_dynamodb::Client;
use std::collections::HashMap;

/// Repository for interacting with DynamoDB.
pub struct DynamoRepository {
    client: Client,
    table_name: String,
}

impl DynamoRepository {
    /// Creates a new DynamoRepository.
    ///
    /// # Arguments
    ///
    /// * `client` - The DynamoDB client.
    /// * `table_name` - The name of the DynamoDB table.
    ///
    /// # Returns
    ///
    /// A new instance of DynamoRepository.
    pub fn new(client: Client, table_name: String) -> Self {
        Self { client, table_name }
    }

    /// Creates a single ExecutionItem in DynamoDB.
    ///
    /// # Arguments
    ///
    /// * `item` - The ExecutionItem to create.
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure.
    pub async fn create_execution_item(&self, item: &ExecutionItem) -> Result<()> {
        let mut request = self
            .client
            .put_item()
            .table_name(&self.table_name)
            .item("PK", AttributeValue::S(item.execution_id.clone()))
            .item("SK", AttributeValue::S(item.guid.to_string()));

        if let Some(title) = &item.title {
            request = request.item("title", AttributeValue::S(title.clone()));
        }

        if let Some(description) = &item.description {
            request = request.item("description", AttributeValue::S(description.clone()))
        }

        if let Some(link) = &item.link {
            request = request.item("link", AttributeValue::S(link.clone()))
        }

        if let Some(ttl) = &item.ttl {
            request = request.item("ttl", AttributeValue::N(ttl.to_string()))
        }

        if let Some(_type) = &item._type {
            request = request.item("_TYPE", AttributeValue::S("ExecutionItem".to_string()))
        }

        if let Some(pub_date) = &item.pub_date {
            request = request.item("pub_date", AttributeValue::S(pub_date.clone()));
        }

        request
            .send()
            .await
            .context("Failed to create execution item")?;
        Ok(())
    }

    /// Creates multiple ExecutionItems in DynamoDB using BatchWriteItem.
    ///
    /// # Arguments
    ///
    /// * `items` - A slice of ExecutionItems to create.
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure.
    pub async fn create_execution_items(&self, items: &[ExecutionItem]) -> Result<()> {
        for chunk in items.chunks(25) {
            let mut write_requests = Vec::new();

            for item in chunk {
                let mut put_request_builder = PutRequest::builder()
                    .item("PK", AttributeValue::S(item.execution_id.clone()))
                    .item("SK", AttributeValue::S(item.guid.to_string()));

                if let Some(title) = &item.title {
                    put_request_builder =
                        put_request_builder.item("title", AttributeValue::S(title.clone()));
                }

                if let Some(description) = &item.description {
                    put_request_builder = put_request_builder
                        .item("description", AttributeValue::S(description.clone()));
                }

                if let Some(link) = &item.link {
                    put_request_builder =
                        put_request_builder.item("link", AttributeValue::S(link.clone()));
                }

                if let Some(ttl) = &item.ttl {
                    put_request_builder =
                        put_request_builder.item("ttl", AttributeValue::N(ttl.to_string()));
                }

                if let Some(_type) = &item._type {
                    put_request_builder = put_request_builder
                        .item("_TYPE", AttributeValue::S("ExecutionItem".to_string()));
                }

                if let Some(pub_date) = &item.pub_date {
                    put_request_builder =
                        put_request_builder.item("pub_date", AttributeValue::S(pub_date.clone()));
                }

                let put_request = put_request_builder
                    .build()
                    .context("Unable to create put_request")?;

                let write_request = WriteRequest::builder().put_request(put_request).build();
                write_requests.push(write_request);
            }

            let mut request_items = HashMap::new();
            request_items.insert(self.table_name.clone(), write_requests);

            let result = self
                .client
                .batch_write_item()
                .set_request_items(Some(request_items))
                .send()
                .await
                .context("Failed to batch write items")?;

            if let Some(unprocessed_items) = result.unprocessed_items() {
                if !unprocessed_items.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Some items were not processed: {:?}",
                        unprocessed_items
                    ));
                }
            }
        }

        Ok(())
    }

    /// Updates the summary of an ExecutionItem.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID of the item to update.
    /// * `guid` - The GUID of the item to update.
    /// * `summary` - The new summary to set.
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure.
    pub async fn update_execution_item_summary(
        &self,
        execution_id: &str,
        guid: &str,
        summary: &str,
    ) -> Result<()> {
        self.client
            .update_item()
            .table_name(&self.table_name)
            .key("PK", AttributeValue::S(execution_id.to_string()))
            .key("SK", AttributeValue::S(guid.to_string()))
            .update_expression("SET summary = :summary")
            .expression_attribute_values(":summary", AttributeValue::S(summary.to_string()))
            .send()
            .await
            .context("Failed to update execution item summary")?;

        Ok(())
    }

    /// Retrieves an ExecutionItem from DynamoDB.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID of the item to retrieve.
    /// * `guid` - The GUID of the item to retrieve.
    ///
    /// # Returns
    ///
    /// A Result containing the ExecutionItem if found, or an error if not found or if the operation failed.
    pub async fn get_execution_item(
        &self,
        execution_id: &str,
        guid: &str,
    ) -> Result<ExecutionItem> {
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("PK", AttributeValue::S(execution_id.to_string()))
            .key("SK", AttributeValue::S(guid.to_string()))
            .send()
            .await
            .context("Failed to get execution item")?;

        if let Some(item) = result.item {
            Ok(ExecutionItem {
                execution_id: execution_id.to_string(),
                guid: guid.to_string(),
                title: item
                    .get("title")
                    .and_then(|av| av.as_s().ok())
                    .map(String::from),
                description: item
                    .get("description")
                    .and_then(|av| av.as_s().ok())
                    .map(String::from),
                link: item
                    .get("link")
                    .and_then(|av| av.as_s().ok())
                    .map(String::from),
                summary: item
                    .get("summary")
                    .and_then(|av| av.as_s().ok())
                    .map(String::from),
                ttl: item
                    .get("ttl")
                    .and_then(|av| av.as_n().ok())
                    .and_then(|n| n.parse().ok()),
                _type: item
                    .get("_TYPE")
                    .and_then(|av| av.as_s().ok())
                    .map(String::from),
                pub_date: item
                    .get("pub_date")
                    .and_then(|av| av.as_s().ok())
                    .map(String::from),
            })
        } else {
            Err(anyhow::anyhow!("Execution item not found"))
        }
    }

    /// Deletes all items with the given execution ID (PK) from DynamoDB.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID (PK) of the items to delete.
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure. On success, returns the number of items deleted.
    pub async fn delete_items_by_execution_id(&self, execution_id: &str) -> Result<u32> {
        let mut items_to_delete = Vec::new();
        let mut last_evaluated_key = None;
        let mut total_deleted = 0;

        // Query for all items with the given PK
        loop {
            let mut query = self
                .client
                .query()
                .table_name(&self.table_name)
                .key_condition_expression("PK = :pk_val")
                .expression_attribute_values(
                    ":pk_val",
                    AttributeValue::S(execution_id.to_string()),
                );

            if let Some(key) = last_evaluated_key {
                query = query.set_exclusive_start_key(Some(key));
            }

            let result = query
                .send()
                .await
                .context("Failed to query items for deletion")?;

            if let Some(items) = result.items {
                for item in items {
                    if let (Some(pk), Some(sk)) = (item.get("PK"), item.get("SK")) {
                        items_to_delete.push((pk.clone(), sk.clone()));
                    }
                }
            }

            last_evaluated_key = result.last_evaluated_key;

            if last_evaluated_key.is_none() {
                break;
            }
        }

        // Delete items in batches of 25 (DynamoDB limit)
        for chunk in items_to_delete.chunks(25) {
            let mut delete_requests = Vec::new();

            for (pk, sk) in chunk {
                let delete_request = DeleteRequest::builder()
                    .key("PK", pk.clone())
                    .key("SK", sk.clone())
                    .build()
                    .context("Unable to create delete_request")?;

                let write_request = WriteRequest::builder()
                    .delete_request(delete_request)
                    .build();

                delete_requests.push(write_request);
            }

            let mut request_items = HashMap::new();
            request_items.insert(self.table_name.clone(), delete_requests);

            let result = self
                .client
                .batch_write_item()
                .set_request_items(Some(request_items))
                .send()
                .await
                .context("Failed to delete items")?;

            total_deleted += chunk.len() as u32;

            if let Some(unprocessed_items) = result.unprocessed_items() {
                if !unprocessed_items.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Some items were not deleted: {:?}",
                        unprocessed_items
                    ));
                }
            }
        }

        Ok(total_deleted)
    }

    /// Creates a RecordItem in DynamoDB.
    ///
    /// # Arguments
    ///
    /// * `item` - The RecordItem to create.
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure.
    pub async fn create_record_item(&self, item: &RecordItem) -> Result<()> {
        self.client
            .put_item()
            .table_name(&self.table_name)
            .item("PK", AttributeValue::S(item.guid.to_string()))
            .item("SK", AttributeValue::S("A".to_string()))
            .item("_TYPE", AttributeValue::S("RecordItem".to_string()))
            .send()
            .await
            .context("Failed to create record item")?;

        Ok(())
    }

    /// Retrieves a RecordItem from DynamoDB.
    ///
    /// # Arguments
    ///
    /// * `guid` - The GUID of the RecordItem to retrieve.
    ///
    /// # Returns
    ///
    /// A Result containing the RecordItem if found, or an error if not found or if the operation failed.
    pub async fn get_record_item(&self, guid: &str) -> Result<RecordItem> {
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("PK", AttributeValue::S(guid.to_string()))
            .key("SK", AttributeValue::S("A".to_string()))
            .send()
            .await
            .context("Failed to get record item")?;

        if let Some(item) = result.item {
            Ok(RecordItem {
                guid: item
                    .get("PK")
                    .and_then(|av| av.as_s().ok())
                    .map(String::from)
                    .context("Missing or invalid guid")?,
                _type: item
                    .get("_TYPE")
                    .and_then(|av| av.as_s().ok())
                    .map(String::from),
            })
        } else {
            Err(anyhow::anyhow!("Record item not found"))
        }
    }

    /// Checks if a RecordItem exists in DynamoDB.
    ///
    /// # Arguments
    ///
    /// * `guid` - The GUID of the RecordItem to check.
    ///
    /// # Returns
    ///
    /// A Result containing a boolean: true if the item exists, false if it doesn't.
    /// Returns an error if the operation failed.
    pub async fn record_item_exists(&self, guid: &str) -> Result<bool> {
        let result = self
            .client
            .get_item()
            .table_name(&self.table_name)
            .key("PK", AttributeValue::S(guid.to_string()))
            .key("SK", AttributeValue::S("A".to_string()))
            .projection_expression("PK") // We only need to retrieve the PK attribute
            .send()
            .await
            .context("Failed to check record item existence")?;

        Ok(result.item.is_some())
    }
}
