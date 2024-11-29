use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::{Client, Error};
use chrono::{Duration, Utc};
use std::collections::HashMap;

use crate::models::{ItemIdentifier, RssItem};

pub async fn store_item_in_dynamodb(
    client: &Client,
    table_name: &str,
    execution_id: &str,
    item: &RssItem,
) -> Result<(), Error> {
    let ttl = Utc::now() + Duration::hours(24);
    let ttl_timestamp = ttl.timestamp();

    let mut item_data = HashMap::new();
    item_data.insert(
        "PK".to_string(),
        AttributeValue::S(execution_id.to_string()),
    );
    item_data.insert("SK".to_string(), AttributeValue::S(item.guid.clone()));
    item_data.insert(
        "TTL".to_string(),
        AttributeValue::N(ttl_timestamp.to_string()),
    );
    item_data.insert("title".to_string(), AttributeValue::S(item.title.clone()));
    item_data.insert(
        "description".to_string(),
        AttributeValue::S(item.description.clone()),
    );
    item_data.insert("link".to_string(), AttributeValue::S(item.link.clone()));
    item_data.insert(
        "pub_date".to_string(),
        AttributeValue::S(item.pub_date.clone()),
    );

    client
        .put_item()
        .table_name(table_name)
        .set_item(Some(item_data))
        .send()
        .await?;

    Ok(())
}

pub async fn check_guid_exists(
    client: &Client,
    table_name: &str,
    guid: &str,
) -> Result<bool, Error> {
    let result = client
        .get_item()
        .table_name(table_name)
        .key("PK", AttributeValue::S(format!("guid-{}", guid)))
        .key("SK", AttributeValue::S("A".to_string()))
        .send()
        .await?;

    Ok(result.item().is_some())
}

pub async fn batch_write_items(
    client: &Client,
    table_name: &str,
    items: &[ItemIdentifier],
) -> Result<(), Error> {
    // DynamoDB allows a maximum of 25 items per batch write
    for chunk in items.chunks(25) {
        let mut request_items = HashMap::new();
        let put_requests: Vec<_> = chunk
            .iter()
            .map(|item| {
                let mut item_data = HashMap::new();
                item_data.insert(
                    "PK".to_string(),
                    AttributeValue::S(format!("guid-{}", item.guid)),
                );
                item_data.insert("SK".to_string(), AttributeValue::S("A".to_string()));
                item_data.insert(
                    "execution_id".to_string(),
                    AttributeValue::S(item.execution_id.clone()),
                );

                aws_sdk_dynamodb::model::WriteRequest::builder()
                    .put_request(
                        aws_sdk_dynamodb::model::PutRequest::builder()
                            .set_item(Some(item_data))
                            .build(),
                    )
                    .build()
            })
            .collect();

        request_items.insert(table_name.to_string(), put_requests);

        client
            .batch_write_item()
            .set_request_items(Some(request_items))
            .send()
            .await?;
    }

    Ok(())
}
