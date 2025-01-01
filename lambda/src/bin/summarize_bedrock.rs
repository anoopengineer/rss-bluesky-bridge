use ::tracing::instrument;
use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use rss_bluesky_bridge::text_utils::truncate_to_word;
use rss_bluesky_bridge::{models::ItemIdentifier, repository::DynamoRepository};
use serde::{Deserialize, Serialize};
use std::env;
use tracing_subscriber::EnvFilter;
use unicode_segmentation::UnicodeSegmentation;

const MAX_BSKY_GRAPHEMES: usize = 290;

#[derive(Deserialize)]
struct Input {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
}

#[derive(Serialize)]
struct Output {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
}

struct Config {
    dynamodb_table_name: String,
    enable_ai_summary: bool,
    ai_model_id: String,
    ai_summary_max_graphemes: i64,
}

impl Config {
    fn from_env() -> Result<Self, Error> {
        let dynamodb_table_name = std::env::var("DYNAMODB_TABLE_NAME")
            .context("DYNAMODB_TABLE_NAME environment variable not set")?;

        if dynamodb_table_name.is_empty() {
            return Err(Error::from("DYNAMODB_TABLE_NAME cannot be empty"));
        }

        let enable_ai_summary = env::var("ENABLE_AI_SUMMARY")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let ai_summary_max_graphemes: i64 = std::env::var("AI_SUMMARY_MAX_GRAPHEMES")
            .context("AI_SUMMARY_MAX_GRAPHEMES environment variable not set")
            .map_err(Error::from)?
            .parse()
            .context("Failed to parse AI_SUMMARY_MAX_GRAPHEMES as an integer")
            .map_err(Error::from)?;

        let ai_model_id: String = env::var("AI_MODEL_ID")
            .context("AI_MODEL_ID environment variable not set")
            .map_err(Error::from)?;

        if enable_ai_summary && ai_model_id.trim().is_empty() {
            return Err(Error::from(
                "AI Summary is enabled, but AI_MODEL_ID env variable is missing",
            ));
        }

        let ai_summary_max_graphemes = if ai_summary_max_graphemes <= 0 {
            if enable_ai_summary {
                tracing::warn!(
                    "AI Summary is enabled, but AI_SUMMARY_MAX_GRAPHEMES is invalid , defaulting to {}. Orignal value = {}", MAX_BSKY_GRAPHEMES,
                    ai_summary_max_graphemes
                );
            }
            280
        } else {
            ai_summary_max_graphemes
        };

        Ok(Self {
            dynamodb_table_name,
            enable_ai_summary,
            ai_model_id,
            ai_summary_max_graphemes,
        })
    }
}

#[instrument(skip(event, repo, bedrock_client, config))]
async fn summarize_bedrock(
    event: LambdaEvent<Input>,
    repo: &DynamoRepository,
    bedrock_client: &BedrockClient,
    config: &Config,
) -> Result<Output, Error> {
    if !config.enable_ai_summary {
        return Ok(Output {
            item_identifier: event.payload.item_identifier,
        });
    }

    // Retrieve item data from DynamoDB
    let item = repo
        .get_execution_item(
            &event.payload.item_identifier.execution_id,
            &event.payload.item_identifier.guid,
        )
        .await
        .with_context(|| {
            format!(
                "Failed to get item from DynamoDB for execution-id {:?} and guid {:?}",
                event.payload.item_identifier.execution_id, event.payload.item_identifier.guid
            )
        })?;

    let description = item.description.context("Description not found in item")?;
    //get the summary from description
    // Prepare the prompt
    let prompt = format!(
    "\n\nHuman: Remove all html tags and summarize the following text in {} graphemes or less:\n\n{}\n\nAssistant:",
    config.ai_summary_max_graphemes, description
);
    tracing::info!("Prompt: {:?}", prompt);

    // Prepare the request body
    let request_body = serde_json::json!({
        "anthropic_version": "bedrock-2023-05-31",
        "max_tokens": 300,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": prompt
                    }
                ]
            }
        ],
        "temperature": 0.0,
        "top_p": 0,
    });

    // Convert the request body to bytes
    let request_body_bytes = serde_json::to_vec(&request_body)?;

    // Make the API call to Bedrock
    let response = bedrock_client
        .invoke_model()
        .body(aws_sdk_bedrockruntime::primitives::Blob::new(
            request_body_bytes,
        ))
        .model_id(&config.ai_model_id)
        .content_type("application/json")
        .accept("application/json")
        .send()
        .await?;

    tracing::info!("Response received: {:?}", response);

    // Parse the response
    let response_body: serde_json::Value = serde_json::from_slice(response.body.as_ref())?;
    tracing::info!("Parsed response body: {:?}", response_body);
    let summary = response_body["content"][0]["text"]
        .as_str()
        .unwrap_or(&description);

    tracing::info!("Summary before trimming:\n{}", summary);
    let summary = truncate_to_word(summary, MAX_BSKY_GRAPHEMES);

    tracing::info!("Summary after trimming:\n{}", summary);
    let num_graphemes = summary.graphemes(true).count();
    tracing::info!(
        "Number of graphemes after trimming in summary: {}",
        num_graphemes
    );

    // Update the DynamoDB entry with the new summary
    repo.update_execution_item_summary(
        &event.payload.item_identifier.execution_id,
        &event.payload.item_identifier.guid,
        &summary,
    )
    .await
    .context("Failed to update item in DynamoDB with summary")?;

    Ok(Output {
        item_identifier: event.payload.item_identifier,
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let config = Config::from_env().expect("Failed to load configuration");
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let dynamodb_client = DynamoDbClient::new(&aws_config);
    let bedrock_client = BedrockClient::new(&aws_config);
    let repo = DynamoRepository::new(dynamodb_client, config.dynamodb_table_name.clone());
    run(service_fn(|event: LambdaEvent<Input>| {
        summarize_bedrock(event, &repo, &bedrock_client, &config)
    }))
    .await
}
