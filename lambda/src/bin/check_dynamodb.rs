use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::Client;
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use rss_bluesky_bridge::{models::ItemIdentifier, repository::DynamoRepository};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use tracing_subscriber::EnvFilter;

#[derive(Deserialize)]
struct Input {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
}

#[derive(Serialize)]
struct Output {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
    should_process: bool,
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
async fn check_dynamodb(
    event: LambdaEvent<Input>,
    repo: &DynamoRepository,
) -> Result<Output, Error> {
    let guid = event.payload.item_identifier.guid.clone();
    tracing::info!("Checking DynamoDB for guid: {}", guid);

    let guid_exists = repo
        .record_item_exists(&guid)
        .await
        .with_context(|| format!("Failed to check if guid exists in DynamoDB: {}", guid))?;

    let output = Output {
        item_identifier: event.payload.item_identifier,
        should_process: !guid_exists,
    };

    tracing::info!(
        "Check result: should_process is {} for guid {}",
        output.should_process,
        guid
    );

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

    let repo = DynamoRepository::new(dynamodb_client, config.dynamodb_table_name);

    run(service_fn(|event: LambdaEvent<Input>| {
        check_dynamodb(event, &repo)
    }))
    .await
}
