#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';
import { RssBlueskyBridgeStack } from '../lib/rss-bluesky-bridge-stack';
import { validatedConfig as config } from './config';

const app = new cdk.App();

console.log(
  `Deploying to ${config.envName} environment. Account = ${config.account}, region = ${config.region}`
);

new RssBlueskyBridgeStack(app, 'RssBlueskyBridgeStack', {
  env: {
    account: config.account,
    region: config.region,
  },
  ...config,
});
