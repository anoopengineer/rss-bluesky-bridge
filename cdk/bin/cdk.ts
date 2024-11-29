#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';
import { RssBlueskyBridgeStack } from '../lib/rss-bluesky-bridge-stack';
import * as dotenv from 'dotenv';

// Load environment variables
dotenv.config({ path: '.env.local' });
const envName = process.env.ENV || 'dev';
const account =
  process.env[`${envName.toUpperCase()}_ACCOUNT`] ||
  process.env.CDK_DEFAULT_ACCOUNT;
const region =
  process.env[`${envName.toUpperCase()}_REGION`] ||
  process.env.CDK_DEFAULT_REGION;

const feedUrl = process.env.FEED_URL;
const maxAgeHours = process.env.MAX_AGE_HOURS;

const enableAISummary = process.env.ENABLE_AI_SUMMARY?.toLowerCase() === 'true';
const aiModelId = process.env.AI_MODEL_ID || '';
const aiSummaryMaxGraphemes = parseInt(
  process.env.AI_SUMMARY_MAX_GRAPHEMES || '100',
  10
);

if (!feedUrl || !maxAgeHours) {
  throw new Error('FEED_URL and MAX_AGE_HOURS must be set in .env.local');
}

if (enableAISummary && !aiModelId) {
  throw new Error(
    'AI summarization is enabled but AI_MODEL_ID is not set. An e.g. value for AI_MODEL_ID is anthropic.claude-3-haiku-20240307-v1:0'
  );
}

if (isNaN(aiSummaryMaxGraphemes) || aiSummaryMaxGraphemes <= 0) {
  throw new Error('AI_SUMMARY_MAX_GRAPHEMES must be a positive integer');
}

const app = new cdk.App();

console.log(
  `Deploying to ${envName} environment. Account = ${account}, region = ${region}`
);

new RssBlueskyBridgeStack(app, 'RssBlueskyBridgeStack', {
  env: {
    account,
    region,
  },
  feedUrl,
  maxAgeHours,
  enableAISummary,
  aiModelId,
  aiSummaryMaxGraphemes,
});
