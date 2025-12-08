//! TAK Server TCP/SSL transport implementation

use async_trait::async_trait;
use hive_protocol::cot::{CotEncoder, CotEvent, CotEventBuilder, CotPoint, CotType};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use super::config::{TakProtocolVersion, TakTransportConfig, TakTransportMode};
use super::error::TakError;
use super::metrics::{QueueDepthMetrics, TakMetrics};
use super::queue::TakMessageQueue;
use super::reconnect::ReconnectionManager;
use super::traits::{CotEventStream, CotFilter, Priority, TakTransport};

/// TAK Protocol magic byte
const TAK_MAGIC: u8 = 0xBF;

/// TAK Server TCP/SSL transport implementation
pub struct TakServerTransport {
    config: TakTransportConfig,
    address: SocketAddr,
    use_tls: bool,
    connection: RwLock<Option<TcpStream>>,
    connected: AtomicBool,
    queue: std::sync::RwLock<TakMessageQueue>,
    reconnect: std::sync::RwLock<ReconnectionManager>,
    metrics: Arc<TakMetrics>,
    #[allow(dead_code)] // Used for protobuf encoding in future
    encoder: CotEncoder,
    /// Channel for incoming events
    #[allow(dead_code)] // Used for subscription impl in future
    incoming_tx: Option<mpsc::Sender<CotEvent>>,
}

impl TakServerTransport {
    /// Create a new TAK Server transport
    pub fn new(config: TakTransportConfig) -> Result<Self, TakError> {
        let (address, use_tls) = match &config.mode {
            TakTransportMode::TakServer { address, use_tls } => (*address, *use_tls),
            TakTransportMode::Hybrid {
                server_address,
                server_use_tls,
                ..
            } => (*server_address, *server_use_tls),
            _ => {
                return Err(TakError::InvalidConfig(
                    "TakServerTransport requires TakServer or Hybrid mode".into(),
                ))
            }
        };

        let queue = TakMessageQueue::new(config.queue.clone());
        let reconnect = ReconnectionManager::new(config.reconnect.clone());
        let encoder = CotEncoder::default();

        Ok(Self {
            config,
            address,
            use_tls,
            connection: RwLock::new(None),
            connected: AtomicBool::new(false),
            queue: std::sync::RwLock::new(queue),
            reconnect: std::sync::RwLock::new(reconnect),
            metrics: Arc::new(TakMetrics::new()),
            encoder,
            incoming_tx: None,
        })
    }

    /// Connect to TAK server (internal)
    async fn establish_connection(&self) -> Result<TcpStream, TakError> {
        info!("Connecting to TAK server at {}", self.address);

        let stream = TcpStream::connect(self.address)
            .await
            .map_err(|e| TakError::ConnectionFailed(format!("TCP connect failed: {}", e)))?;

        // Disable Nagle's algorithm for lower latency
        stream.set_nodelay(true).ok();

        if self.use_tls {
            // TODO: Implement TLS handshake
            warn!("TLS not yet implemented, using plain TCP");
        }

        info!("Connected to TAK server at {}", self.address);
        Ok(stream)
    }

    /// Send presence announcement
    async fn send_presence(&self, stream: &mut TcpStream) -> Result<(), TakError> {
        let callsign = self
            .config
            .identity
            .as_ref()
            .map(|i| i.callsign.as_str())
            .unwrap_or("HIVE-BRIDGE");

        let uid = format!("HIVE-{}", uuid::Uuid::new_v4());
        // Default position at 0,0 for presence announcement
        // Real position would come from configuration or GPS
        let presence = CotEventBuilder::new()
            .uid(&uid)
            .cot_type(CotType::new("a-f-G-U-C"))
            .how("m-g")
            .point(CotPoint::new(0.0, 0.0))
            .build()
            .map_err(|e| {
                TakError::EncodingError(format!("Failed to build presence event: {}", e))
            })?;

        debug!("Sending presence as '{}'", callsign);
        self.send_event_raw(stream, &presence).await
    }

    /// Send a CoT event on the given stream
    async fn send_event_raw(
        &self,
        stream: &mut TcpStream,
        event: &CotEvent,
    ) -> Result<(), TakError> {
        let xml = event
            .to_xml()
            .map_err(|e| TakError::EncodingError(format!("XML encoding failed: {}", e)))?;
        let payload = xml.as_bytes();

        // Frame the message based on protocol version
        let frame = match self.config.protocol.version {
            TakProtocolVersion::RawXml => {
                // Raw XML, no framing - for FreeTAKServer
                payload.to_vec()
            }
            TakProtocolVersion::XmlTcp => {
                // XML framing: [0xBF][0x00][0xBF][payload]
                let mut frame = Vec::with_capacity(3 + payload.len());
                frame.push(TAK_MAGIC);
                frame.push(0x00); // Version 0 = XML
                frame.push(TAK_MAGIC);
                frame.extend_from_slice(payload);
                frame
            }
            TakProtocolVersion::ProtobufV1 => {
                // For now, use XML in Protobuf framing
                // TODO: Implement actual Protobuf encoding
                // Protobuf framing: [0xBF][varint_length][payload]
                let mut frame = Vec::with_capacity(1 + 5 + payload.len());
                frame.push(TAK_MAGIC);
                Self::encode_varint(payload.len() as u64, &mut frame);
                frame.extend_from_slice(payload);
                frame
            }
        };

        stream.write_all(&frame).await.map_err(TakError::IoError)?;

        self.metrics.record_send(frame.len());
        debug!("Sent CoT event: {} ({} bytes)", event.uid, frame.len());

        Ok(())
    }

    /// Encode a value as a varint
    fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
        while value >= 0x80 {
            buf.push((value as u8 & 0x7F) | 0x80);
            value >>= 7;
        }
        buf.push(value as u8);
    }

    /// Drain queued messages after reconnection
    async fn drain_queue(&self, stream: &mut TcpStream) -> Result<usize, TakError> {
        let mut sent = 0;
        loop {
            let msg = {
                let mut queue = self.queue.write().unwrap();
                queue.dequeue()
            };

            match msg {
                Some(queued) => {
                    if let Err(e) = self.send_event_raw(stream, &queued.event).await {
                        // Re-queue the message
                        let mut queue = self.queue.write().unwrap();
                        let _ = queue.enqueue(queued.event, queued.priority);
                        return Err(e);
                    }
                    sent += 1;
                }
                None => break,
            }
        }

        if sent > 0 {
            info!("Drained {} queued messages", sent);
        }

        Ok(sent)
    }
}

#[async_trait]
impl TakTransport for TakServerTransport {
    async fn connect(&mut self) -> Result<(), TakError> {
        let mut stream = self.establish_connection().await?;

        // Send initial presence
        self.send_presence(&mut stream).await?;

        // Drain any queued messages
        self.drain_queue(&mut stream).await?;

        // Store connection
        *self.connection.write().await = Some(stream);
        self.connected.store(true, Ordering::SeqCst);
        self.metrics.record_connect();
        self.reconnect.write().unwrap().reset();

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), TakError> {
        // Take stream out of lock before awaiting
        let stream = { self.connection.write().await.take() };
        if let Some(mut stream) = stream {
            info!("Disconnecting from TAK server");
            let _ = stream.shutdown().await;
        }

        self.connected.store(false, Ordering::SeqCst);
        self.metrics.record_disconnect();

        Ok(())
    }

    async fn send_cot(&self, event: &CotEvent, priority: Priority) -> Result<(), TakError> {
        if self.is_connected() {
            // Try to send directly - use async lock
            let mut guard = self.connection.write().await;
            if let Some(stream) = guard.as_mut() {
                match self.send_event_raw(stream, event).await {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        error!("Send failed, queueing message: {}", e);
                        self.metrics.record_error(&e.to_string());
                        // Fall through to queue
                    }
                }
            }
        }

        // Queue for later
        let mut queue = self.queue.write().unwrap();
        queue.enqueue(event.clone(), priority)?;
        debug!("Queued CoT event {} (priority {})", event.uid, priority);

        Ok(())
    }

    async fn subscribe(&self, _filter: CotFilter) -> Result<CotEventStream, TakError> {
        // TODO: Implement subscription with background reader task
        Err(TakError::NotConnected)
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn metrics(&self) -> TakMetrics {
        (*self.metrics).clone()
    }

    fn queue_depth(&self) -> QueueDepthMetrics {
        self.queue.read().unwrap().metrics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_encoding() {
        let mut buf = Vec::new();
        TakServerTransport::encode_varint(0, &mut buf);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        TakServerTransport::encode_varint(127, &mut buf);
        assert_eq!(buf, vec![0x7F]);

        buf.clear();
        TakServerTransport::encode_varint(128, &mut buf);
        assert_eq!(buf, vec![0x80, 0x01]);

        buf.clear();
        TakServerTransport::encode_varint(300, &mut buf);
        assert_eq!(buf, vec![0xAC, 0x02]);
    }

    #[test]
    fn test_new_server_transport() {
        let config = TakTransportConfig {
            mode: TakTransportMode::TakServer {
                address: "127.0.0.1:8087".parse().unwrap(),
                use_tls: false,
            },
            ..Default::default()
        };

        let transport = TakServerTransport::new(config);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_new_server_transport_wrong_mode() {
        let config = TakTransportConfig {
            mode: TakTransportMode::MeshSa {
                multicast_group: "239.2.3.1".parse().unwrap(),
                port: 6969,
                interface: None,
            },
            ..Default::default()
        };

        let transport = TakServerTransport::new(config);
        assert!(transport.is_err());
    }
}
