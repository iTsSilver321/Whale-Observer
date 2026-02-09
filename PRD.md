🐳 Project: The Moby Dick Observer (PRD)
Objective: Build a high-performance, async Rust bot that monitors the Ethereum blockchain via WebSocket, detects "Whale" swaps (>$50k USD) on Uniswap V3, and sends instant Telegram alerts.

1. The "No-Fail" Tech Stack
Language: Rust (Latest Stable)

Blockchain Client: alloy (The new industry standard, replacing ethers).

Async Runtime: tokio (For handling massive concurrency).

Database: sqlx with SQLite (Fast, local storage to remember whales).

Alerting: teloxide (Telegram Bot API).

Environment: .env file for API keys.

2. The Setup (Copy-Paste this into Cursor first)
User Action: Open your terminal, run cargo new moby_dick, open the folder in Cursor, and create a file named .env.

Prompt 1 (The Foundation):

I am building a high-performance blockchain monitor in Rust. I need you to set up the Cargo.toml with the latest industry-standard crates.

Please add the following dependencies:

alloy with features ["full"] (for connecting to Ethereum WSS).

tokio with features ["full"] (for async runtime).

teloxide with features ["macros"] (for Telegram).

dotenv (to load secrets).

serde and serde_json (for parsing data).

anyhow (for error handling).

tracing and tracing-subscriber (for logging, do not use println!).

Generate the Cargo.toml content and a main.rs that simply initializes the logger and loads the .env file.

3. Phase 1: The Connection (WebSocket)
Context: We need to connect to Alchemy without crashing. If the internet blips, it must reconnect automatically.

Prompt 2 (The Listener):

Now, write the connection logic in main.rs.

Load ALCHEMY_WSS_URL from .env.

Use alloy::providers::ProviderBuilder to connect via WebSocket (WsConnect).

Create a subscription to Logs (not Blocks).

Use alloy::rpc::types::Filter to listen to the Uniswap V3 USDC/ETH Pool address: 0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640.

In the loop, just print "Log Received" for now.

Crucial: Wrap the connection logic in a loop. If the WebSocket disconnects, wait 5 seconds and retry. Do not let the bot panic and exit.

4. Phase 2: The Decrypter (Parsing Uniswap)
Context: Blockchain data is binary (hex). We need to turn it into readable numbers.

Prompt 3 (The Logic):

I need to decode the logs. The Uniswap V3 Swap event signature is Swap(address,address,int256,int256,uint160,uint128,int24).

Define a sol! macro struct using alloy::sol! to generate the event bindings for event Swap(address indexed sender, address indexed recipient, int256 amount0, int256 amount1, uint160 sqrtPriceX96, uint128 liquidity, int24 tick);.

Inside the listener loop, decode the log into this Struct.

Extract amount0 (USDC) and amount1 (WETH).

Note: amount1 is the WETH amount. Check if the absolute value of amount1 is greater than 20 * 10^18 (20 ETH).

If it is, print "WHALE DETECTED" with the amount.

5. Phase 3: The Town Crier (Telegram)
Context: You can't watch the terminal 24/7. Send it to your phone.

User Action: Get a Bot Token from @BotFather on Telegram and your Chat ID from @userinfobot. Add them to .env.

Prompt 4 (The Alert):

Implement the Telegram alerting system.

Initialize teloxide::Bot from the TELEGRAM_BOT_TOKEN env var.

Create a separate async function send_alert(bot: &Bot, message: String).

When a "Whale" is detected in the main loop, format a message: "🚨 Whale Alert! 🚨 Amount: [Amount] ETH Tx Hash: [Link to Etherscan]"

Call send_alert. Ensure you use tokio::spawn for the alert so sending the message doesn't block the next blockchain event processing.

6. Phase 4: Production Hardening (The "No Fail" Step)
Context: The bot will eventually crash due to "RPC Limit" or "weird data." We need to armor it.

Prompt 5 (Final Polish):

Review the code for stability.

Rate Limiting: Ensure we don't send more than 1 Telegram message per second (use a simplified leaky bucket or simple timestamp check).

Error Handling: Ensure that if log.decode() fails (due to a malformed log), we print an error trace but continue the loop. The bot must strictly NEVER panic.

Logging: Replace all println! with tracing::info! or tracing::error!.

Your Homework (The "Vibe Coder" Checklist)
Get the Keys:

Alchemy: Sign up for free, create an App (Ethereum Mainnet), copy the WebSocket (wss://) URL.

Telegram: Message @BotFather -> /newbot -> Copy Token.

Run the Prompts: Copy the prompts above into Cursor one by one. Read the code it generates.

Run the Bot:

Bash
cargo run
Pro Tip: If the bot isn't firing, the Uniswap Pool address I gave you (0x88e6...) is the USDC/ETH pool. If the market is quiet, change the filter to "All Logs" for a minute just to test if the connection works.