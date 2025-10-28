//! CAP Protocol Simulator
//!
//! Reference implementation for simulating and visualizing the CAP protocol.

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("CAP Protocol Simulator starting...");
    info!("Version: {}", cap_protocol::VERSION);

    // TODO: Implement simulation harness
    println!("CAP Protocol Simulator v{}", cap_protocol::VERSION);
    println!("Ready for implementation!");

    Ok(())
}
