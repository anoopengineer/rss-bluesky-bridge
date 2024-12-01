use ::tracing::instrument;
use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::Client as BedrockClient;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use rss_bluesky_bridge::models::ItemIdentifier;
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

#[instrument(skip(event, dynamodb_client, bedrock_client, config))]
async fn summarize_bedrock(
    event: LambdaEvent<Input>,
    dynamodb_client: &DynamoDbClient,
    bedrock_client: &BedrockClient,
    config: &Config,
) -> Result<Output, Error> {
    if !config.enable_ai_summary {
        return Ok(Output {
            item_identifier: event.payload.item_identifier,
        });
    }

    //get the ddb entry
    // Retrieve item data from DynamoDB
    let result = dynamodb_client
        .get_item()
        .table_name(&config.dynamodb_table_name)
        .key(
            "PK",
            aws_sdk_dynamodb::types::AttributeValue::S(
                event.payload.item_identifier.execution_id.clone(),
            ),
        )
        .key(
            "SK",
            aws_sdk_dynamodb::types::AttributeValue::S(event.payload.item_identifier.guid.clone()),
        )
        .send()
        .await
        .context("Failed to get item from DynamoDB")
        .map_err(Error::from)?;

    let item = result
        .item()
        .context("Item not found in DynamoDB")
        .map_err(Error::from)?;

    let description = item
        .get("description")
        .and_then(|v| v.as_s().ok())
        .context("Description not found in item")
        .map_err(Error::from)?;

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
        .unwrap_or("No summary generated");

    tracing::info!("Summary before trimming:\n{}", summary);
    let num_graphemes = summary.graphemes(true).count();
    tracing::info!(
        "Number of graphemes before trimming in summary: {}",
        num_graphemes
    );

    // Trim the summary to 300 graphemes
    let summary = if num_graphemes > MAX_BSKY_GRAPHEMES {
        let mut graphemes = summary.graphemes(true).collect::<Vec<&str>>();
        while graphemes.len() > MAX_BSKY_GRAPHEMES - 1 {
            // -1 so that we can accommodate the ellipsis
            graphemes.pop();
        }

        // Find the last space to avoid cutting words
        let mut last_space_index = graphemes.len();
        while last_space_index > 0 && graphemes[last_space_index - 1] != " " {
            last_space_index -= 1;
        }

        // If we found a space, use it as the cut-off point
        if last_space_index > 0 {
            graphemes.truncate(last_space_index);
        }

        let mut trimmed = graphemes.join("");
        trimmed.push('…'); // Add ellipsis
        trimmed
    } else {
        summary.to_string()
    };

    tracing::info!("Summary after trimming:\n{}", summary);
    let num_graphemes = summary.graphemes(true).count();
    tracing::info!(
        "Number of graphemes after trimming in summary: {}",
        num_graphemes
    );

    //update the ddb entry with a new "summary" column

    let update_result = dynamodb_client
        .update_item()
        .table_name(&config.dynamodb_table_name)
        .key(
            "PK",
            aws_sdk_dynamodb::types::AttributeValue::S(
                event.payload.item_identifier.execution_id.clone(),
            ),
        )
        .key(
            "SK",
            aws_sdk_dynamodb::types::AttributeValue::S(event.payload.item_identifier.guid.clone()),
        )
        .update_expression("SET summary = :summary")
        .expression_attribute_values(
            ":summary",
            aws_sdk_dynamodb::types::AttributeValue::S(summary.to_string()),
        )
        .send()
        .await
        .context("Failed to update item in DynamoDB with summary")
        .map_err(Error::from)?;

    tracing::info!("DynamoDB update result: {:?}", update_result);

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
    run(service_fn(|event: LambdaEvent<Input>| {
        summarize_bedrock(event, &dynamodb_client, &bedrock_client, &config)
    }))
    .await
}
