use ::tracing::instrument;
use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::Client;
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use rss_bluesky_bridge::{
    models::{ItemIdentifier, RecordItem},
    repository::DynamoRepository,
};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

#[derive(Deserialize)]
struct Input {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
}

#[derive(Serialize, Debug)]
struct Output {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
}

struct Config {
    dynamodb_table_name: String,
}

impl Config {
    fn from_env() -> Result<Self, Error> {
        let dynamodb_table_name = std::env::var("DYNAMODB_TABLE_NAME")
            .context("DYNAMODB_TABLE_NAME environment variable not set")?;

        if dynamodb_table_name.is_empty() {
            return Err(Error::from("DYNAMODB_TABLE_NAME cannot be empty"));
        }

        Ok(Self {
            dynamodb_table_name,
        })
    }
}
#[instrument(skip(event, repo))]
async fn update_dynamodb(
    event: LambdaEvent<Input>,
    repo: &DynamoRepository,
) -> Result<Output, Error> {
    let record_item = RecordItem::new(event.payload.item_identifier.guid.clone())
        .context("Failed to create RecordItem")?;

    repo.create_record_item(&record_item)
        .await
        .context("Failed to create record item in DynamoDB")?;

    let output = Output {
        item_identifier: event.payload.item_identifier,
    };

    tracing::info!("Update result: {:?}", output);
    Ok(output)
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

    run(service_fn(|event: LambdaEvent<Input>| {
        update_dynamodb(event, &repo)
    }))
    .await
}
