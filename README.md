# RSS-Bluesky Bridge

Welcome to the RSS-Bluesky Bridge project! This open-source tool automatically fetches RSS feed items and posts them to Bluesky, creating a seamless bridge between traditional RSS feeds and the Bluesky social network.

## 🌟 Features

- Fetches items from an RSS feed
- Filters items based on age
- Optionally summarizes content using AI (via Amazon Bedrock)
- Posts items to Bluesky with rich text and external link embeds
- Prevents duplicate posts using DynamoDB
- Serverless architecture using AWS CDK and Lambda functions

## 🛠 Tech Stack

- AWS CDK (TypeScript)
- AWS Lambda (Rust)
- Amazon DynamoDB
- Amazon Bedrock (optional, for AI summarization)
- AWS Step Functions
- AWS Secrets Manager to store Bluesky username and password

## 🚀 Getting Started

### Prerequisites

- [AWS CLI](https://aws.amazon.com/cli/) configured with appropriate permissions
- [Node.js](https://nodejs.org/) (v22 or later)
- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)
- [cargo-lambda](https://github.com/cargo-lambda/cargo-lambda)

### Setup

1. Clone the repository:

    
```bash
git clone https://github.com/yourusername/rss-bluesky-bridge.git
cd rss-bluesky-bridge
```
    

2. Install dependencies:

    
```bash
cd cdk
npm install
```
    

3. Create a `.env.local` file in the `cdk` directory with the following content:
    
```
DEV_ACCOUNT=your-aws-account-id
DEV_REGION=your-aws-region

PROD_ACCOUNT=123456789012
PROD_REGION=us-west-2

FEED_URL=hhttps://example.com/feed.rss
MAX_AGE_HOURS=48

ENABLE_AI_SUMMARY=true
AI_MODEL_ID=anthropic.claude-3-haiku-20240307-v1:0
AI_SUMMARY_MAX_GRAPHEMES=100
```
    

4. Deploy the stack:

```bash    
cd cdk
ENV=prod npx cdk deploy
```
    

5. After deployment, go to the AWS Secrets Manager console and update the `bluesky-credentials` secret with your Bluesky username and password:
```json
{
  "username": "your-bluesky-username",
  "password": "your-bluesky-password"
}

    

🔧 Configuration

    FEED_URL: The URL of the RSS feed you want to bridge to Bluesky
    MAX_AGE_HOURS: Maximum age of RSS items to consider (in hours)
    ENABLE_AI_SUMMARY: Set to true to enable AI summarization using Amazon Bedrock
    AI_MODEL_ID: The Bedrock model ID to use for summarization
    AI_SUMMARY_MAX_GRAPHEMES: Maximum length of AI-generated summaries

🤝 Contributing

We welcome contributions to the RSS-Bluesky Bridge project! Here's how you can help:

    Fork the repository
    Create a new branch (git checkout -b feature/amazing-feature)
    Make your changes
    Commit your changes (git commit -am 'Add some amazing feature')
    Push to the branch (git push origin feature/amazing-feature)
    Open a Pull Request

Please make sure to update tests as appropriate and adhere to the existing coding style.
📜 License

This project is licensed under the MIT License - see the [LICENSE](https://console.harmony.a2z.com/LICENSE) file for details.
🙏 Acknowledgements

    [AWS CDK](https://aws.amazon.com/cdk/)
    [Rust](https://www.rust-lang.org/)
    [Bluesky](https://bsky.app/)
    [RSS](https://en.wikipedia.org/wiki/RSS)

📞 Support

If you have any questions or need help with setup, please open an issue in the GitHub repository, and we'll be happy to assist you!

Happy bridging! 🌉