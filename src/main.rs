use alloy::{
    primitives::{address, U256},
    providers::{Provider, ProviderBuilder, WsConnect},
    rpc::types::Filter,
    sol,
};
use anyhow::Result;
use futures_util::StreamExt;
use std::sync::Arc;
use teloxide::{prelude::*, types::ChatId};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{error, info, warn};

// === GENERATED BINDINGS ===
// The sol! macro reads the Solidity event signature and generates
// Rust types with encode/decode methods automatically.
sol! {
    #[allow(missing_docs)]
    event Swap(
        address indexed sender,
        address indexed recipient,
        int256 amount0,
        int256 amount1,
        uint160 sqrtPriceX96,
        uint128 liquidity,
        int24 tick
    );
}

/// The whale threshold: 20 ETH (in wei). 1 ETH = 10^18 wei.
const WHALE_THRESHOLD_WEI: u128 = 20 * 1_000_000_000_000_000_000;

/// Minimum interval between Telegram messages (rate limiting).
const MIN_ALERT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging
    tracing_subscriber::fmt::init();

    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    info!("🐳 The Moby Dick Observer is starting up...");

    // Load config from environment
    let wss_url = std::env::var("ALCHEMY_WSS_URL")
        .expect("ALCHEMY_WSS_URL must be set in .env");
    let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
        .expect("TELEGRAM_BOT_TOKEN must be set in .env");
    let chat_id: i64 = std::env::var("TELEGRAM_CHAT_ID")
        .expect("TELEGRAM_CHAT_ID must be set in .env")
        .parse()
        .expect("TELEGRAM_CHAT_ID must be a valid integer");

    // Initialize Telegram bot
    let bot = Bot::new(&bot_token);
    let chat_id = ChatId(chat_id);

    // Rate limiter: tracks the last time we sent a Telegram message.
    // Arc<Mutex<...>> allows safe sharing across spawned tasks.
    let last_alert_time = Arc::new(Mutex::new(Instant::now() - MIN_ALERT_INTERVAL));

    info!("✅ Environment loaded. Bot initialized. Connecting to Ethereum...");

    // === RECONNECTION LOOP ===
    // If the WebSocket disconnects, we wait 5 seconds and retry.
    // The bot NEVER panics and exits.
    loop {
        match run_listener(&wss_url, &bot, chat_id, &last_alert_time).await {
            Ok(()) => {
                info!("🔌 Stream ended. Reconnecting in 5 seconds...");
            }
            Err(e) => {
                error!("❌ Connection error: {e:#}. Reconnecting in 5 seconds...");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

/// Core listener — connects via WebSocket, subscribes to Uniswap V3
/// USDC/ETH pool logs, decodes Swap events, and detects whales.
async fn run_listener(
    wss_url: &str,
    bot: &Bot,
    chat_id: ChatId,
    last_alert_time: &Arc<Mutex<Instant>>,
) -> Result<()> {
    // Connect to Ethereum via WebSocket
    let ws = WsConnect::new(wss_url);
    let provider = ProviderBuilder::new().connect_ws(ws).await?;

    info!("🔗 Connected to Ethereum via WebSocket!");

    // Uniswap V3 USDC/ETH Pool address
    let pool_address = address!("88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640");

    // Create a filter for the Swap event on this pool
    let filter = Filter::new()
        .address(pool_address)
        .event("Swap(address,address,int256,int256,uint160,uint128,int24)");

    // Subscribe to matching logs
    let sub = provider.subscribe_logs(&filter).await?;
    let mut stream = sub.into_stream();

    info!("👁️ Listening for swaps on Uniswap V3 USDC/ETH pool...");

    // Process each log as it arrives
    while let Some(log) = stream.next().await {
        let tx_hash = log
            .transaction_hash
            .map(|h| format!("{h:#x}"))
            .unwrap_or_else(|| "unknown".to_string());

        // Try to decode the log into our Swap struct.
        // If decoding fails (malformed log), we log a warning and continue.
        let swap = match log.log_decode::<Swap>() {
            Ok(decoded) => decoded.inner.data,
            Err(e) => {
                warn!("⚠️ Failed to decode log: {e}. Tx: {tx_hash}. Skipping...");
                continue;
            }
        };

        // amount1 is WETH. We need the absolute value.
        let abs_amount1 = get_abs_wei(&swap.amount1);

        // Convert to a human-readable ETH amount (divide by 10^18)
        let eth_amount = abs_amount1 as f64 / 1e18;

        if abs_amount1 >= WHALE_THRESHOLD_WEI {
            info!(
                "🐋 WHALE DETECTED! {:.4} ETH | Tx: https://etherscan.io/tx/{}",
                eth_amount, tx_hash
            );

            // === RATE LIMITING ===
            // Check if enough time has passed since the last alert.
            let mut last_time = last_alert_time.lock().await;
            if last_time.elapsed() >= MIN_ALERT_INTERVAL {
                *last_time = Instant::now();
                drop(last_time); // Release lock before spawning

                // Determine direction:
                // amount1 is WETH.
                // If amount1 is negative, the pool lost WETH (User BOUGHT ETH).
                // If amount1 is positive, the pool gained WETH (User SOLD ETH).
                let (action_emoji, action_text) = if swap.amount1.is_negative() {
                    ("🟢", "BOUGHT")
                } else {
                    ("🔴", "SOLD")
                };

                // MarkdownV2 requires escaping: . - ( ) ! = |
                let eth_str = format!("{:.4}", eth_amount).replace('.', "\\.");
                
                let message = format!(
                    "{action_emoji} *WHALE {action_text} \\!* {action_emoji}\n\n\
                     💰 Amount: *{eth_str} ETH*\n\
                     🔗 [View on Etherscan](https://etherscan.io/tx/{tx_hash})"
                );

                info!(
                    "{action_emoji} WHALE {action_text}! {:.4} ETH | Tx: https://etherscan.io/tx/{}",
                    eth_amount, tx_hash
                );

                // Send Telegram alert in a separate task so it doesn't
                // block the next blockchain event from being processed.
                let bot_clone = bot.clone();
                tokio::spawn(async move {
                    send_alert(&bot_clone, chat_id, &message).await;
                });
            } else {
                info!("⏳ Rate limited — skipping Telegram alert for this whale.");
            }
        } else {
            info!("🐟 Small swap: {:.4} ETH | Tx: {tx_hash}", eth_amount);
        }
    }

    Ok(())
}

/// Sends a Telegram alert message. Handles errors gracefully (logs, no panic).
async fn send_alert(bot: &Bot, chat_id: ChatId, message: &str) {
    match bot
        .send_message(chat_id, message)
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await
    {
        Ok(_) => info!("📨 Telegram alert sent successfully!"),
        Err(e) => error!("❌ Failed to send Telegram alert: {e}"),
    }
}

/// Extracts the absolute value of an int256 as u128.
/// Swap amounts can be negative (indicating direction), so we take abs.
fn get_abs_wei(value: &alloy::primitives::I256) -> u128 {
    let abs_value = if value.is_negative() {
        value.wrapping_neg()
    } else {
        *value
    };

    // Convert I256 -> U256 -> u128 (safe because ETH amounts fit in u128)
    let as_u256: U256 = abs_value.into_raw();
    as_u256.to::<u128>()
}
