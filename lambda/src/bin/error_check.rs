use ::tracing::instrument;
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

#[derive(Deserialize)]
struct ProcessedItem {
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct Input {
    processed_items: Vec<ProcessedItem>,
}

#[derive(Serialize, Debug)]
struct Output {
    has_errors: bool,
    error_count: usize,
    total_items: usize,
}

#[instrument(skip(event))]
async fn error_check(event: LambdaEvent<Input>) -> Result<Output, Error> {
    tracing::info!("Checking for errors in processed items");

    let total_items = event.payload.processed_items.len();
    let error_count = event
        .payload
        .processed_items
        .iter()
        .filter(|item| item.error.is_some())
        .count();
    tracing::info!(
        "total_items = {} error_count = {}",
        total_items,
        error_count
    );

    let output = Output {
        has_errors: error_count > 0,
        error_count,
        total_items,
    };

    tracing::info!("Error check result: {:?}", output);
    Ok(output)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    run(service_fn(error_check)).await
}
