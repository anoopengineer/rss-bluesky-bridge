import * as dotenv from 'dotenv';

// Load environment variables
dotenv.config({ path: '.env.local' });

const env = process.env.ENV || 'dev';

const config = {
  envName: env,
  account:
    process.env[`${env.toUpperCase()}_ACCOUNT`] ||
    process.env.CDK_DEFAULT_ACCOUNT ||
    '',
  region:
    process.env[`${env.toUpperCase()}_REGION`] ||
    process.env.CDK_DEFAULT_REGION ||
    '',
  feedUrl: process.env.FEED_URL || '',
  maxAgeHours: process.env.MAX_AGE_HOURS || '',
  enableAISummary: process.env.ENABLE_AI_SUMMARY?.toLowerCase() === 'true',
  aiModelId: process.env.AI_MODEL_ID || '',
  aiSummaryMaxGraphemes: parseInt(
    process.env.AI_SUMMARY_MAX_GRAPHEMES || '100',
    10
  ),
  logLevel: process.env.RUST_LOG || 'trace',
};

// Validation and assertions
if (!config.feedUrl) {
  throw new Error('FEED_URL must be set in .env.local');
}

if (!config.maxAgeHours) {
  throw new Error('MAX_AGE_HOURS must be set in .env.local');
}

if (!config.account) {
  throw new Error(
    'AWS account must be set in .env.local or provided by AWS CLI configuration'
  );
}

if (!config.region) {
  throw new Error(
    'AWS region must be set in .env.local or provided by AWS CLI configuration'
  );
}

if (config.enableAISummary && !config.aiModelId) {
  throw new Error('AI summarization is enabled but AI_MODEL_ID is not set.');
}

if (isNaN(config.aiSummaryMaxGraphemes) || config.aiSummaryMaxGraphemes <= 0) {
  throw new Error('AI_SUMMARY_MAX_GRAPHEMES must be a positive integer');
}

// Final validated config
export const validatedConfig = {
  ...config,
  account: config.account as string,
  region: config.region as string,
  feedUrl: config.feedUrl as string,
  maxAgeHours: config.maxAgeHours as string,
} as const;

// Type assertion to ensure all properties are non-undefined
export type Config = typeof validatedConfig;
