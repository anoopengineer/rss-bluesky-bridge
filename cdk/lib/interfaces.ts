import * as cdk from 'aws-cdk-lib';

export interface RssBlueskyBridgeStackProps extends cdk.StackProps {
  feedUrl: string;
  maxAgeHours: string;
  enableAISummary: boolean;
  aiModelId: string;
  aiSummaryMaxGraphemes: number;
  logLevel: string;
}
