use ::tracing::instrument;
use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use aws_sdk_dynamodb::Client;
use chrono::{DateTime, Duration, Utc};
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use reqwest;
use rss::Channel;
use rss_bluesky_bridge::{
    models::{ExecutionItem, ItemIdentifier},
    repository::DynamoRepository,
};
use serde::Serialize;
use std::env;
use tracing_subscriber::EnvFilter;

#[derive(Serialize)]
struct Output {
    item_identifiers: Vec<ItemIdentifier>,
}

struct Config {
    dynamodb_table_name: String,
    max_age_hours: i64,
    feed_url: String,
}

impl Config {
    fn from_env() -> Result<Self, Error> {
        let dynamodb_table_name = std::env::var("DYNAMODB_TABLE_NAME")
            .context("DYNAMODB_TABLE_NAME environment variable not set")?;

        if dynamodb_table_name.is_empty() {
            return Err(Error::from("DYNAMODB_TABLE_NAME cannot be empty"));
        }

        let max_age_hours: i64 = std::env::var("MAX_AGE_HOURS")
            .context("MAX_AGE_HOURS environment variable not set")
            .map_err(Error::from)?
            .parse()
            .context("Failed to parse MAX_AGE_HOURS as an integer")
            .map_err(Error::from)?;

        let max_age_hours = if max_age_hours <= 0 {
            tracing::warn!(
                "MAX_AGE_HOURS is set to 0 or less, defaulting to 24 hours. Orignal value = {}",
                max_age_hours
            );
            48
        } else {
            max_age_hours
        };

        let feed_url: String = env::var("FEED_URL")
            .context("FEED_URL environment variable not set")
            .map_err(Error::from)?;

        if feed_url.trim().is_empty() {
            return Err(Error::from("FEED_URL is not provided"));
        }

        Ok(Self {
            dynamodb_table_name,
            max_age_hours,
            feed_url,
        })
    }
}

#[instrument(skip(event, repo, config))]
async fn get_rss_items(
    event: LambdaEvent<CloudWatchEvent>,
    repo: &DynamoRepository,
    config: &Config,
) -> Result<Output, Error> {
    tracing::info!("Payload: {:?}", event.payload);
    let execution_id = event
        .payload
        .id
        .ok_or_else(|| Error::from("Execution ID not provided in the event payload"))?;
    tracing::info!("Execution id: {:?}", execution_id);

    let content = reqwest::get(&config.feed_url)
        .await
        .with_context(|| format!("Failed to fetch RSS feed from {}", config.feed_url))
        .map_err(Error::from)?
        .text()
        .await
        .context("Failed to read RSS feed content")
        .map_err(Error::from)?;

    let channel = Channel::read_from(content.as_bytes())
        .context("Failed to parse RSS feed")
        .map_err(Error::from)?;

    let ttl = Utc::now() + Duration::hours(24);
    let ttl_timestamp = ttl.timestamp();

    let (execution_items, item_identifiers): (Vec<ExecutionItem>, Vec<ItemIdentifier>) = channel
        .items()
        .iter()
        .filter_map(|item| {
            let pub_date = item.pub_date()?;
            let pub_date = DateTime::parse_from_rfc2822(&pub_date).ok()?;
            let age = Utc::now().signed_duration_since(pub_date);

            if age.num_hours() <= config.max_age_hours {
                let guid = item.guid()?.value().to_string();
                Some((
                    ExecutionItem {
                        execution_id: execution_id.clone(),
                        guid: guid.clone(),
                        title: item.title().map(String::from),
                        description: item.description().map(String::from),
                        link: item.link().map(String::from),
                        summary: None,
                        ttl: Some(ttl_timestamp),
                        _type: None,
                        pub_date: Some(pub_date.to_rfc2822()),
                    },
                    ItemIdentifier {
                        execution_id: execution_id.clone(),
                        guid,
                    },
                ))
            } else {
                None
            }
        })
        .unzip();

    // Store items in DynamoDB using bulk API
    repo.create_execution_items(&execution_items).await?;

    Ok(Output { item_identifiers })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env().expect("Failed to load configuration");
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let dynamodb_client = Client::new(&aws_config);
    let repo = DynamoRepository::new(dynamodb_client, config.dynamodb_table_name.clone());

    run(service_fn(|event: LambdaEvent<CloudWatchEvent>| {
        get_rss_items(event, &repo, &config)
    }))
    .await
}
