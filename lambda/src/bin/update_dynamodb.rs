use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::{types::AttributeValue, Client};
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
struct ItemIdentifier {
    execution_id: String,
    guid: String,
}

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

async fn update_dynamodb(event: LambdaEvent<Input>) -> Result<Output, Error> {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    let table_name = std::env::var("DYNAMODB_TABLE_NAME")
        .context("DYNAMODB_TABLE_NAME environment variable not set")
        .map_err(Error::from)?;

    client
        .put_item()
        .table_name(table_name)
        .item(
            "PK",
            AttributeValue::S(format!(
                "guid-{}",
                event.payload.item_identifier.guid.clone()
            )),
        )
        .item("SK", AttributeValue::S("A".to_string()))
        .send()
        .await
        .context("Failed to put item in DynamoDB")
        .map_err(Error::from)?;

    let output = Output {
        item_identifier: event.payload.item_identifier,
    };

    tracing::info!("Error check result: {:?}", output);
    Ok(output)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();
    run(service_fn(update_dynamodb)).await
}
