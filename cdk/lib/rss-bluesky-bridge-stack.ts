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
import { RssBlueskyBridgeStackProps } from './interfaces';

export class RssBlueskyBridgeStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: RssBlueskyBridgeStackProps) {
    super(scope, id, props);

    const blueskySecret = this.createBlueskySecret();
    const table = this.createDynamoDbTable();

    const lambdas = {
      getRssItems: this.createLambdaFunction(
        'GetRssItemsLambda',
        'get-rss-items',
        {
          FEED_URL: props.feedUrl,
          DYNAMODB_TABLE_NAME: table.tableName,
          MAX_AGE_HOURS: props.maxAgeHours,
          RUST_LOG: props.logLevel,
        }
      ),
      checkDynamoDb: this.createLambdaFunction(
        'CheckDynamoDbLambda',
        'check-dynamodb',
        {
          DYNAMODB_TABLE_NAME: table.tableName,
          RUST_LOG: props.logLevel,
        }
      ),
      summarizeBedrock: this.createLambdaFunction(
        'SummarizeBedrockLambda',
        'summarize-bedrock',
        {
          DYNAMODB_TABLE_NAME: table.tableName,
          ENABLE_AI_SUMMARY: String(props.enableAISummary),
          AI_MODEL_ID: props.aiModelId,
          AI_SUMMARY_MAX_GRAPHEMES: String(props.aiSummaryMaxGraphemes),
          RUST_LOG: props.logLevel,
        }
      ),
      postBluesky: this.createLambdaFunction(
        'PostBlueskyLambda',
        'post-bluesky',
        {
          BLUESKY_CREDENTIALS_SECRET_NAME: blueskySecret.secretName,
          DYNAMODB_TABLE_NAME: table.tableName,
          RUST_LOG: props.logLevel,
        }
      ),
      updateDynamoDb: this.createLambdaFunction(
        'UpdateDynamoDbLambda',
        'update-dynamodb',
        {
          DYNAMODB_TABLE_NAME: table.tableName,
          RUST_LOG: props.logLevel,
        }
      ),
      errorCheck: this.createLambdaFunction('ErrorCheckLambda', 'error-check', {
        RUST_LOG: props.logLevel,
      }),
    };

    // Set up permissions
    table.grantWriteData(lambdas.getRssItems);
    table.grantReadData(lambdas.checkDynamoDb);
    table.grantReadWriteData(lambdas.summarizeBedrock);
    blueskySecret.grantRead(lambdas.postBluesky);
    table.grantReadData(lambdas.postBluesky);
    table.grantWriteData(lambdas.updateDynamoDb);

    lambdas.summarizeBedrock.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['bedrock:InvokeModel'],
        resources: [`arn:aws:bedrock:${this.region}::foundation-model/*`],
      })
    );

    const stateMachine = this.createStateMachine(lambdas);
    this.createScheduleRule(stateMachine);
  }

  private createBlueskySecret(): secretsmanager.Secret {
    return new secretsmanager.Secret(this, 'BlueskyCredentials', {
      secretName: 'bluesky-credentials',
      description: 'Credentials for Bluesky API used by Rss-Bluesky-Bridge',
      generateSecretString: {
        secretStringTemplate: JSON.stringify({
          username: 'dummy_username',
          password: 'dummy_password',
        }),
        generateStringKey: 'dummy',
      },
    });
  }

  private createDynamoDbTable(): dynamodb.Table {
    return new dynamodb.Table(this, 'RssBlueskyBridgeTable', {
      partitionKey: { name: 'PK', type: dynamodb.AttributeType.STRING },
      sortKey: { name: 'SK', type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      timeToLiveAttribute: 'TTL',
    });
  }

  private createLambdaFunction(
    id: string,
    binaryName: string,
    environment: Record<string, string>
  ): RustFunction {
    return new RustFunction(this, id, {
      manifestPath: path.join(__dirname, '../../lambda'),
      binaryName,
      environment,
      architecture: Architecture.ARM_64,
    });
  }

  private createStateMachine(
    lambdas: Record<string, RustFunction>
  ): sfn.StateMachine {
    const getRssItems = new tasks.LambdaInvoke(this, 'GetRSSFeedItems', {
      lambdaFunction: lambdas.getRssItems,
      resultPath: '$.items',
    });

    const checkDynamoDB = new tasks.LambdaInvoke(this, 'CheckDynamoDB', {
      lambdaFunction: lambdas.checkDynamoDb,
      payloadResponseOnly: true,
    });

    const summarizeBedrock = new tasks.LambdaInvoke(this, 'SummarizeBedrock', {
      lambdaFunction: lambdas.summarizeBedrock,
      payloadResponseOnly: true,
    });

    const postToBluesky = new tasks.LambdaInvoke(this, 'PostToBluesky', {
      lambdaFunction: lambdas.postBluesky,
      payloadResponseOnly: true,
    });

    const updateDynamoDB = new tasks.LambdaInvoke(this, 'UpdateDynamoDB', {
      lambdaFunction: lambdas.updateDynamoDb,
      payloadResponseOnly: true,
    });

    const shouldProcess = new sfn.Choice(this, 'ShouldProcess')
      .when(
        sfn.Condition.booleanEquals('$.should_process', true),
        summarizeBedrock.next(postToBluesky).next(updateDynamoDB)
      )
      .otherwise(new sfn.Pass(this, 'SkipProcessing'));

    const processItem = checkDynamoDB.next(shouldProcess);

    const processItems = new sfn.Map(this, 'ProcessItems', {
      itemsPath: '$.items.Payload.item_identifiers',
      resultPath: '$.processed_items',
      maxConcurrency: 1,
    }).iterator(processItem);

    const errorCheck = new tasks.LambdaInvoke(this, 'ErrorCheck', {
      lambdaFunction: lambdas.errorCheck,
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

    return new sfn.StateMachine(this, 'RssBlueskyBridgeStateMachine', {
      definition: getRssItems
        .next(processItems)
        .next(errorCheck)
        .next(finalErrorCheck),
    });
  }

  private createScheduleRule(stateMachine: sfn.StateMachine): void {
    new events.Rule(this, 'ScheduleRule', {
      schedule: events.Schedule.cron({ minute: '0', hour: '*/6' }),
      targets: [new targets.SfnStateMachine(stateMachine)],
    });
  }
}
