//! Event ingestion from stdin, TCP, or file.
//!
//! Reads JSON lines from the configured source, parses them into
//! ViewerEvents, applies them to the state buffer, and broadcasts
//! to connected WebSocket clients.

use crate::relay::broadcast::Broadcaster;
use crate::relay::buffer::StateBuffer;
use crate::ws::protocol::IngestLine;
use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Ingest source configuration.
#[derive(Debug, Clone)]
pub enum IngestSource {
    /// Read JSON lines from stdin (pipe from simulation).
    Stdin,
    /// Read JSON lines from a TCP socket.
    Tcp(String),
    /// Read JSON lines from a file (replay mode).
    File(String),
}

impl IngestSource {
    /// Parse a source string into an IngestSource.
    /// Formats: "stdin", "tcp://host:port", "file://path"
    pub fn parse(s: &str) -> Result<Self> {
        if s == "stdin" {
            Ok(Self::Stdin)
        } else if let Some(addr) = s.strip_prefix("tcp://") {
            Ok(Self::Tcp(addr.to_string()))
        } else if let Some(path) = s.strip_prefix("file://") {
            Ok(Self::File(path.to_string()))
        } else {
            anyhow::bail!("Unknown ingest source: {}. Use 'stdin', 'tcp://host:port', or 'file://path'", s);
        }
    }
}

/// Run the ingest loop. Reads lines from the source, parses, buffers, broadcasts.
/// This function runs until the source is exhausted or an error occurs.
pub async fn run_ingest(
    source: IngestSource,
    buffer: StateBuffer,
    broadcaster: Broadcaster,
) -> Result<()> {
    match source {
        IngestSource::Stdin => ingest_stdin(buffer, broadcaster).await,
        IngestSource::Tcp(addr) => ingest_tcp(&addr, buffer, broadcaster).await,
        IngestSource::File(path) => ingest_file(&path, buffer, broadcaster).await,
    }
}

async fn ingest_stdin(buffer: StateBuffer, broadcaster: Broadcaster) -> Result<()> {
    tracing::info!("Ingesting from stdin");
    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    ingest_lines(reader, buffer, broadcaster).await
}

async fn ingest_tcp(addr: &str, buffer: StateBuffer, broadcaster: Broadcaster) -> Result<()> {
    tracing::info!("Listening for TCP ingest on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind TCP ingest on {}", addr))?;

    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::info!("TCP ingest connection from {}", peer);
        let reader = BufReader::new(stream);
        let buf = buffer.clone();
        let bc = broadcaster.clone();

        // Handle each TCP connection in a separate task.
        // When the connection closes, we continue accepting new ones.
        tokio::spawn(async move {
            if let Err(e) = ingest_lines(reader, buf, bc).await {
                tracing::warn!("TCP ingest connection ended: {}", e);
            }
        });
    }
}

async fn ingest_file(path: &str, buffer: StateBuffer, broadcaster: Broadcaster) -> Result<()> {
    tracing::info!("Ingesting from file: {}", path);
    let file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("Failed to open file: {}", path))?;
    let reader = BufReader::new(file);
    ingest_lines(reader, buffer, broadcaster).await?;
    tracing::info!("File ingest complete: {}", path);
    Ok(())
}

async fn ingest_lines<R: tokio::io::AsyncRead + Unpin>(
    reader: BufReader<R>,
    buffer: StateBuffer,
    broadcaster: Broadcaster,
) -> Result<()> {
    let mut lines = reader.lines();
    let mut line_count: u64 = 0;
    let mut classified_count: u64 = 0;

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        line_count += 1;

        // Try to parse as JSON
        let ingest_line: IngestLine = match serde_json::from_str(&line) {
            Ok(l) => l,
            Err(e) => {
                tracing::trace!("Skipping non-JSON line {}: {}", line_count, e);
                continue;
            }
        };

        // Classify into a ViewerEvent
        if let Some(event) = ingest_line.classify() {
            classified_count += 1;
            buffer.apply(&event).await;

            // Broadcast to connected clients (ignore if no receivers)
            let _ = broadcaster.send(&event);

            if classified_count % 50 == 0 {
                tracing::debug!(
                    "Ingested {} lines, {} classified, {} viewers",
                    line_count,
                    classified_count,
                    broadcaster.receiver_count(),
                );
            }
        }
    }

    tracing::info!(
        "Ingest complete: {} lines read, {} events classified",
        line_count,
        classified_count,
    );
    Ok(())
}
