import * as cdk from 'aws-cdk-lib';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import * as sfn from 'aws-cdk-lib/aws-stepfunctions';
import * as tasks from 'aws-cdk-lib/aws-stepfunctions-tasks';
import * as events from 'aws-cdk-lib/aws-events';
import * as targets from 'aws-cdk-lib/aws-events-targets';
import * as dynamodb from 'aws-cdk-lib/aws-dynamodb';
import { RustFunction } from 'cargo-lambda-cdk';
import { Construct } from 'constructs';
import * as path from 'path';
import * as iam from 'aws-cdk-lib/aws-iam';
import { Architecture } from 'aws-cdk-lib/aws-lambda';

interface RssBlueskyBridgeStackProps extends cdk.StackProps {
  feedUrl: string;
  maxAgeHours: string;
  enableAISummary: boolean;
  aiModelId: string;
  aiSummaryMaxGraphemes: number;
}

export class RssBlueskyBridgeStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: RssBlueskyBridgeStackProps) {
    super(scope, id, props);

    // Create the secret with dummy values
    const blueskySecret = new secretsmanager.Secret(
      this,
      'BlueskyCredentials',
      {
        secretName: 'bluesky-credentials',
        description: 'Credentials for Bluesky API used by Rss-Bluesky-Bridge',
        generateSecretString: {
          secretStringTemplate: JSON.stringify({
            username: 'dummy_username',
            password: 'dummy_password',
          }),
          generateStringKey: 'dummy', // This key won't be used, it's just to satisfy the API
        },
      }
    );

    // Create DynamoDB table
    const table = new dynamodb.Table(this, 'RssBlueskyBridgeTable', {
      partitionKey: { name: 'PK', type: dynamodb.AttributeType.STRING },
      sortKey: { name: 'SK', type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      timeToLiveAttribute: 'TTL',
    });

    // Create Lambda functions
    const getRssItemsLambda = new RustFunction(this, 'GetRssItemsLambda', {
      manifestPath: path.join(__dirname, '../../lambda'),
      binaryName: 'get-rss-items',
      environment: {
        FEED_URL: props.feedUrl,
        DYNAMODB_TABLE_NAME: table.tableName,
        MAX_AGE_HOURS: props.maxAgeHours,
      },
      architecture: Architecture.ARM_64,
    });
    table.grantWriteData(getRssItemsLambda);

    const checkDynamoDbLambda = new RustFunction(this, 'CheckDynamoDbLambda', {
      manifestPath: path.join(__dirname, '../../lambda'),
      binaryName: 'check-dynamodb',
      environment: {
        DYNAMODB_TABLE_NAME: table.tableName,
      },
      architecture: Architecture.ARM_64,
    });

    const summarizeBedrockLambda = new RustFunction(
      this,
      'SummarizeBedrockLambda',
      {
        manifestPath: path.join(__dirname, '../../lambda'),
        binaryName: 'summarize-bedrock',
        environment: {
          DYNAMODB_TABLE_NAME: table.tableName,
          ENABLE_AI_SUMMARY: String(props.enableAISummary),
          AI_MODEL_ID: props.aiModelId,
          AI_SUMMARY_MAX_GRAPHEMES: String(props.aiSummaryMaxGraphemes),
        },
        architecture: Architecture.ARM_64,
      }
    );
    table.grantReadWriteData(summarizeBedrockLambda);
    // Add permissions to invoke Bedrock model
    summarizeBedrockLambda.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['bedrock:InvokeModel'],
        resources: [`arn:aws:bedrock:${this.region}::foundation-model/*`],
      })
    );

    const postBlueskyLambda = new RustFunction(this, 'PostBlueskyLambda', {
      manifestPath: path.join(__dirname, '../../lambda'),
      binaryName: 'post-bluesky',
      environment: {
        BLUESKY_CREDENTIALS_SECRET_NAME: blueskySecret.secretName,
        DYNAMODB_TABLE_NAME: table.tableName,
      },
      architecture: Architecture.ARM_64,
    });

    // Grant read access to the secret
    blueskySecret.grantRead(postBlueskyLambda);
    table.grantReadData(postBlueskyLambda);
    const updateDynamoDbLambda = new RustFunction(
      this,
      'UpdateDynamoDbLambda',
      {
        manifestPath: path.join(__dirname, '../../lambda'),
        binaryName: 'update-dynamodb',
        environment: {
          DYNAMODB_TABLE_NAME: table.tableName,
        },
        architecture: Architecture.ARM_64,
      }
    );

    // Grant permissions to Lambda functions
    table.grantReadData(checkDynamoDbLambda);
    table.grantWriteData(updateDynamoDbLambda);

    const errorCheckLambda = new RustFunction(this, 'ErrorCheckLambda', {
      manifestPath: path.join(__dirname, '../../lambda'),
      binaryName: 'error-check',
      architecture: Architecture.ARM_64,
    });

    // Define Step Function tasks
    const getRssItems = new tasks.LambdaInvoke(this, 'GetRSSFeedItems', {
      lambdaFunction: getRssItemsLambda,
      resultPath: '$.items',
    });

    const checkDynamoDB = new tasks.LambdaInvoke(this, 'CheckDynamoDB', {
      lambdaFunction: checkDynamoDbLambda,
      payloadResponseOnly: true,
    });

    const summarizeBedrock = new tasks.LambdaInvoke(this, 'SummarizeBedrock', {
      lambdaFunction: summarizeBedrockLambda,
      payloadResponseOnly: true,
    });

    const postToBluesky = new tasks.LambdaInvoke(this, 'PostToBluesky', {
      lambdaFunction: postBlueskyLambda,
      payloadResponseOnly: true,
    });

    const updateDynamoDB = new tasks.LambdaInvoke(this, 'UpdateDynamoDB', {
      lambdaFunction: updateDynamoDbLambda,
      payloadResponseOnly: true,
    });

    // Define Choice state
    const shouldProcess = new sfn.Choice(this, 'ShouldProcess')
      .when(
        sfn.Condition.booleanEquals('$.should_process', true),
        summarizeBedrock.next(postToBluesky).next(updateDynamoDB)
      )
      .otherwise(new sfn.Pass(this, 'SkipProcessing'));

    // Define the item processing workflow
    const processItem = checkDynamoDB.next(shouldProcess);

    // Define Map state
    const processItems = new sfn.Map(this, 'ProcessItems', {
      itemsPath: '$.items.Payload.item_identifiers',
      resultPath: '$.processed_items',
      maxConcurrency: 1,
    }).iterator(processItem);

    // Define final error check
    const errorCheck = new tasks.LambdaInvoke(this, 'ErrorCheck', {
      lambdaFunction: errorCheckLambda,
      resultPath: '$.errorCheckResult',
    });

    const finalErrorCheck = new sfn.Choice(this, 'FinalErrorCheck')
      .when(
        sfn.Condition.booleanEquals(
          '$.errorCheckResult.Payload.has_errors',
          true
        ),
        new sfn.Fail(this, 'FailExecution', {
          error: 'ItemProcessingFailed',
          cause: 'One or more items failed to process',
        })
      )
      .otherwise(new sfn.Pass(this, 'SuccessfulExecution'));

    // Define the state machine
    const stateMachine = new sfn.StateMachine(
      this,
      'RssBlueskyBridgeStateMachine',
      {
        definition: getRssItems
          .next(processItems)
          .next(errorCheck)
          .next(finalErrorCheck),
      }
    );

    new events.Rule(this, 'ScheduleRule', {
      schedule: events.Schedule.cron({ minute: '0', hour: '*/6' }),
      targets: [new targets.SfnStateMachine(stateMachine)],
    });
  }
}
