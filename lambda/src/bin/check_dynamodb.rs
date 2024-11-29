use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::{types::AttributeValue, Client};
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct ItemIdentifier {
    execution_id: String,
    guid: String,
}

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

async fn check_dynamodb(event: LambdaEvent<Input>) -> Result<Output, Error> {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);
    let table_name = std::env::var("DYNAMODB_TABLE_NAME")
        .context("DYNAMODB_TABLE_NAME environment variable not set")
        .map_err(Error::from)?;

    let guid_result = client
        .get_item()
        .table_name(&table_name)
        .key(
            "PK",
            AttributeValue::S(format!("guid-{}", event.payload.item_identifier.guid)),
        )
        .key("SK", AttributeValue::S("A".to_string()))
        .send()
        .await
        .context("Failed to get response from DynamoDB for guid check")
        .map_err(Error::from)?;

    let guid_exists: bool = guid_result.item().is_some();

    Ok(Output {
        item_identifier: event.payload.item_identifier,
        should_process: !guid_exists,
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(service_fn(check_dynamodb)).await
}
