use anyhow::Context;
use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::Client;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use serde::{Deserialize, Serialize};
use std::env;
use unicode_segmentation::UnicodeSegmentation;

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

#[derive(Serialize)]
struct Output {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
}

const MAX_BSKY_GRAPHEMES: usize = 300;

async fn summarize_bedrock(event: LambdaEvent<Input>) -> Result<Output, Error> {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let bedrock_client = Client::new(&config);
    let dynamodb_client = DynamoDbClient::new(&config);

    let table_name = env::var("DYNAMODB_TABLE_NAME")
        .context("DYNAMODB_TABLE_NAME environment variable not set")
        .map_err(Error::from)?;

    let enable_ai_summary = env::var("ENABLE_AI_SUMMARY")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    if !enable_ai_summary {
        return Ok(Output {
            item_identifier: event.payload.item_identifier,
        });
    }

    let ai_summary_max_graphemes: i64 = std::env::var("AI_SUMMARY_MAX_GRAPHEMES")
        .context("AI_SUMMARY_MAX_GRAPHEMES environment variable not set")
        .map_err(Error::from)?
        .parse()
        .context("Failed to parse AI_SUMMARY_MAX_GRAPHEMES as an integer")
        .map_err(Error::from)?;

    let ai_model_id: String = env::var("AI_MODEL_ID")
        .context("AI_MODEL_ID environment variable not set")
        .map_err(Error::from)?;

    //get the ddb entry
    // Retrieve item data from DynamoDB
    let result = dynamodb_client
        .get_item()
        .table_name(&table_name)
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
    ai_summary_max_graphemes, description
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
        .model_id(ai_model_id)
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
        .table_name(&table_name)
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
    tracing::init_default_subscriber();
    run(service_fn(summarize_bedrock)).await
}
