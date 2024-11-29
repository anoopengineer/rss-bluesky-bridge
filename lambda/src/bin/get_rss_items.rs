use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use aws_sdk_dynamodb::{types::AttributeValue, Client};
use chrono::{DateTime, Duration, Utc};
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use reqwest;
use rss::Channel;
use rss_bluesky_bridge::models::{ItemIdentifier, RssItem};
use serde::Serialize;
use std::env;

#[derive(Serialize)]
struct Output {
    item_identifiers: Vec<ItemIdentifier>,
}

async fn get_rss_items(event: LambdaEvent<CloudWatchEvent>) -> Result<Output, Error> {
    tracing::info!("Payload: {:?}", event.payload);
    let execution_id = event
        .payload
        .id
        .ok_or_else(|| Error::from("Execution ID not provided in the event payload"))?;
    tracing::info!("Execution id: {:?}", execution_id);

    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let dynamodb_client = Client::new(&config);
    let table_name = std::env::var("DYNAMODB_TABLE_NAME")
        .context("DYNAMODB_TABLE_NAME environment variable not set")
        .map_err(Error::from)?;

    let max_age_hours: i64 = std::env::var("MAX_AGE_HOURS")
        .context("MAX_AGE_HOURS environment variable not set")
        .map_err(Error::from)?
        .parse()
        .context("Failed to parse MAX_AGE_HOURS as an integer")
        .map_err(Error::from)?;

    let feed_url: String = env::var("FEED_URL")
        .context("FEED_URL environment variable not set")
        .map_err(Error::from)?;

    let content = reqwest::get(&feed_url)
        .await
        .with_context(|| format!("Failed to fetch RSS feed from {}", feed_url))
        .map_err(Error::from)?
        .text()
        .await
        .context("Failed to read RSS feed content")
        .map_err(Error::from)?;

    let channel = Channel::read_from(content.as_bytes())
        .context("Failed to parse RSS feed")
        .map_err(Error::from)?;

    let items: Vec<RssItem> = channel
        .items()
        .iter()
        .filter_map(|item| {
            let pub_date = item.pub_date()?;
            let pub_date = DateTime::parse_from_rfc2822(&pub_date).ok()?;
            let age = Utc::now().signed_duration_since(pub_date);

            if age.num_hours() <= max_age_hours {
                Some(RssItem {
                    guid: item.guid()?.value().to_string(),
                    title: item.title()?.to_string(),
                    description: item.description()?.to_string(),
                    link: item.link()?.to_string(),
                    pub_date: pub_date.to_rfc2822(),
                })
            } else {
                None
            }
        })
        .collect();

    let mut item_identifiers = Vec::new();

    for item in items {
        let ttl = Utc::now() + Duration::hours(24);
        let ttl_timestamp = ttl.timestamp();

        // Prepare item data
        let mut item_data = std::collections::HashMap::new();
        item_data.insert("PK".to_string(), AttributeValue::S(execution_id.clone()));
        item_data.insert("SK".to_string(), AttributeValue::S(item.guid.clone()));
        item_data.insert(
            "TTL".to_string(),
            AttributeValue::N(ttl_timestamp.to_string()),
        );

        // Add other item fields
        item_data.insert(
            "title".to_string(),
            AttributeValue::S(item.title.to_string()),
        );
        item_data.insert(
            "description".to_string(),
            AttributeValue::S(item.description.to_string()),
        );
        item_data.insert("link".to_string(), AttributeValue::S(item.link.to_string()));
        item_data.insert(
            "pub_date".to_string(),
            AttributeValue::S(item.pub_date.to_string()),
        );

        // Store item in DynamoDB
        dynamodb_client
            .put_item()
            .table_name(&table_name)
            .set_item(Some(item_data))
            .send()
            .await?;

        // Add to list of identifiers
        item_identifiers.push(ItemIdentifier {
            execution_id: execution_id.clone(),
            guid: item.guid.clone(),
        });
    }

    Ok(Output { item_identifiers })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();
    run(service_fn(get_rss_items)).await
}
