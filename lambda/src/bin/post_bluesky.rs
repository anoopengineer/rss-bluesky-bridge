use ::tracing::instrument;
use anyhow::Context;
use atrium_api::app::bsky::embed::external::External;
use atrium_api::app::bsky::embed::external::ExternalData;
use atrium_api::app::bsky::embed::external::Main;
use atrium_api::app::bsky::embed::external::MainData;
use atrium_api::app::bsky::feed::post::RecordEmbedRefs;
use atrium_api::types::Union;
use aws_config::BehaviorVersion;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use bsky_sdk::rich_text::RichText;
use bsky_sdk::BskyAgent;
use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use rss_bluesky_bridge::models::ItemIdentifier;
use serde::{Deserialize, Serialize};
use std::env;
use tracing_subscriber::EnvFilter;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Deserialize)]
struct Input {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
}

#[derive(Serialize)]
struct Output {
    #[serde(flatten)]
    item_identifier: ItemIdentifier,
    uri: String,
}

struct Config {
    dynamodb_table_name: String,
    secret_name: String,
}

impl Config {
    fn from_env() -> Result<Self, Error> {
        let dynamodb_table_name = std::env::var("DYNAMODB_TABLE_NAME")
            .context("DYNAMODB_TABLE_NAME environment variable not set")?;

        if dynamodb_table_name.trim().is_empty() {
            return Err(Error::from("DYNAMODB_TABLE_NAME cannot be empty"));
        }

        let secret_name = env::var("BLUESKY_CREDENTIALS_SECRET_NAME")
            .context("BLUESKY_CREDENTIALS_SECRET_NAME environment variable not set")
            .map_err(Error::from)?;

        if secret_name.trim().is_empty() {
            return Err(Error::from(
                "BLUESKY_CREDENTIALS_SECRET_NAME cannot be empty",
            ));
        }

        Ok(Self {
            dynamodb_table_name,
            secret_name,
        })
    }
}

#[instrument(skip(event, dynamodb_client, config))]
async fn post_bluesky(
    event: LambdaEvent<Input>,
    dynamodb_client: &DynamoDbClient,
    secrets_client: &SecretsManagerClient,
    config: &Config,
) -> Result<Output, Error> {
    tracing::info!(
        "Posting to Bluesky for item: {:?}",
        event.payload.item_identifier
    );

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

    let title = item
        .get("title")
        .and_then(|v| v.as_s().ok())
        .context("Title not found in item")
        .map_err(Error::from)?;
    let description = item
        .get("description")
        .and_then(|v| v.as_s().ok())
        .context("Description not found in item")
        .map_err(Error::from)?;

    let summary = match item.get("summary").and_then(|v| v.as_s().ok()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => {
            tracing::info!("AI generated summary unavailable. Generating summary from description");
            let words: Vec<&str> = description.split_word_bounds().collect();
            let mut char_count = 0;
            let mut word_index = 0;

            while word_index < words.len() && char_count + words[word_index].len() <= 99 {
                char_count += words[word_index].len();
                word_index += 1;
            }

            if word_index < words.len() {
                format!("{}…", words[..word_index].join(""))
            } else {
                description.to_string()
            }
        }
    };
    tracing::info!("Using summary: {}", summary);

    let link = item
        .get("link")
        .and_then(|v| v.as_s().ok())
        .context("Link not found in item")
        .map_err(Error::from)?;

    // Construct the post content
    let post = format!("{}\n\n", summary);
    tracing::info!(
        "Number of graphemes in post: {}",
        post.graphemes(true).count()
    );

    // Get Bluesky credentials
    let secret = secrets_client
        .get_secret_value()
        .secret_id(&config.secret_name)
        .send()
        .await
        .context("Failed to retrieve secret")
        .map_err(Error::from)?;

    let secret_string = secret
        .secret_string()
        .context("Secret string is empty")
        .map_err(Error::from)?;
    let credentials: serde_json::Value = serde_json::from_str(secret_string)
        .context("Failed to parse secret JSON")
        .map_err(Error::from)?;

    let username = credentials["username"]
        .as_str()
        .context("Username not found in secret")
        .map_err(Error::from)?;
    let password = credentials["password"]
        .as_str()
        .context("Password not found in secret")
        .map_err(Error::from)?;

    // Create Bluesky post
    let rt = RichText::new_with_detect_facets(post)
        .await
        .context("Failed to create RichText")
        .map_err(Error::from)?;

    let record_data = atrium_api::app::bsky::feed::post::RecordData {
        created_at: atrium_api::types::string::Datetime::now(),
        embed: Some(Union::Refs(RecordEmbedRefs::AppBskyEmbedExternalMain(
            Box::new(Main {
                data: MainData {
                    external: External {
                        data: ExternalData {
                            title: title.clone(),
                            description: "".to_string(),
                            uri: link.clone(),
                            thumb: None,
                        },
                        extra_data: ipld_core::ipld::Ipld::Null,
                    },
                },
                extra_data: ipld_core::ipld::Ipld::Null,
            }),
        ))),
        entities: None,
        facets: rt.facets,
        labels: None,
        langs: None,
        reply: None,
        tags: None,
        text: rt.text,
    };

    let agent = BskyAgent::builder()
        .build()
        .await
        .context("Failed to build BskyAgent")
        .map_err(Error::from)?;
    agent
        .login(username, password)
        .await
        .context("Failed to login to Bluesky")
        .map_err(Error::from)?;

    let result = agent
        .create_record(record_data)
        .await
        .context("Failed to create Bluesky post")
        .map_err(Error::from)?;

    Ok(Output {
        item_identifier: event.payload.item_identifier,
        uri: result.uri.clone(),
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
    let secrets_client = SecretsManagerClient::new(&aws_config);
    run(service_fn(|event: LambdaEvent<Input>| {
        post_bluesky(event, &dynamodb_client, &secrets_client, &config)
    }))
    .await
}
