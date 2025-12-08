//! TAK Server TCP/SSL transport implementation

use async_trait::async_trait;
use futures::stream;
use hive_protocol::cot::{CotEncoder, CotEvent, CotEventBuilder, CotPoint, CotType};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace, warn};

use super::config::{TakProtocolVersion, TakTransportConfig, TakTransportMode};
use super::error::TakError;
use super::metrics::{QueueDepthMetrics, TakMetrics};
use super::queue::TakMessageQueue;
use super::reconnect::ReconnectionManager;
use super::traits::{CotEventStream, CotFilter, Priority, TakTransport};

/// TAK Protocol magic byte
const TAK_MAGIC: u8 = 0xBF;

/// Buffer size for incoming event channel
const INCOMING_CHANNEL_SIZE: usize = 256;

/// TAK Server TCP/SSL transport implementation
pub struct TakServerTransport {
    config: TakTransportConfig,
    address: SocketAddr,
    use_tls: bool,
    /// Write half of the TCP connection (for sending)
    write_stream: RwLock<Option<OwnedWriteHalf>>,
    connected: AtomicBool,
    queue: std::sync::RwLock<TakMessageQueue>,
    reconnect: std::sync::RwLock<ReconnectionManager>,
    metrics: Arc<TakMetrics>,
    #[allow(dead_code)] // Used for protobuf encoding in future
    encoder: CotEncoder,
    /// Broadcast sender for incoming events (subscribers receive from this)
    incoming_tx: broadcast::Sender<CotEvent>,
    /// Handle to the reader task
    reader_task: RwLock<Option<JoinHandle<()>>>,
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
        let (incoming_tx, _) = broadcast::channel(INCOMING_CHANNEL_SIZE);

        Ok(Self {
            config,
            address,
            use_tls,
            write_stream: RwLock::new(None),
            connected: AtomicBool::new(false),
            queue: std::sync::RwLock::new(queue),
            reconnect: std::sync::RwLock::new(reconnect),
            metrics: Arc::new(TakMetrics::new()),
            encoder,
            incoming_tx,
            reader_task: RwLock::new(None),
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

    /// Send presence announcement on write half of split stream
    async fn send_presence_on_write(&self, stream: &mut OwnedWriteHalf) -> Result<(), TakError> {
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
        stream: &mut OwnedWriteHalf,
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

    /// Spawn the background reader task to receive CoT events from TAK server
    fn spawn_reader_task(
        read_half: OwnedReadHalf,
        incoming_tx: broadcast::Sender<CotEvent>,
        protocol_version: TakProtocolVersion,
        metrics: Arc<TakMetrics>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            let mut buffer = String::new();

            info!("TAK reader task started");

            loop {
                buffer.clear();

                // Read based on protocol version
                let result = match protocol_version {
                    TakProtocolVersion::RawXml => {
                        // For raw XML (FreeTAKServer), read until we find </event>
                        Self::read_raw_xml_event(&mut reader, &mut buffer).await
                    }
                    TakProtocolVersion::XmlTcp | TakProtocolVersion::ProtobufV1 => {
                        // For framed protocols, read based on framing
                        Self::read_framed_event(&mut reader, &mut buffer).await
                    }
                };

                match result {
                    Ok(true) => {
                        // Successfully read an event
                        trace!("Received raw CoT XML: {}", buffer.trim());
                        match CotEvent::from_xml(&buffer) {
                            Ok(event) => {
                                metrics.record_receive(buffer.len());
                                debug!(
                                    "Parsed incoming CoT event: {} (type: {})",
                                    event.uid,
                                    event.cot_type.as_str()
                                );
                                // Send to all subscribers (ignore if no receivers)
                                let _ = incoming_tx.send(event);
                            }
                            Err(e) => {
                                warn!("Failed to parse CoT XML: {}", e);
                                metrics.record_error(&format!("Parse error: {}", e));
                            }
                        }
                    }
                    Ok(false) => {
                        // Connection closed
                        info!("TAK server connection closed");
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from TAK server: {}", e);
                        metrics.record_error(&e.to_string());
                        break;
                    }
                }
            }

            info!("TAK reader task stopped");
        })
    }

    /// Read a raw XML CoT event (for FreeTAKServer)
    async fn read_raw_xml_event(
        reader: &mut BufReader<OwnedReadHalf>,
        buffer: &mut String,
    ) -> Result<bool, TakError> {
        // Read until we get </event> end tag
        // This is a simplified approach - real TAK servers may send multiple events
        loop {
            let bytes_read = reader.read_line(buffer).await.map_err(TakError::IoError)?;

            if bytes_read == 0 {
                return Ok(false); // EOF
            }

            // Check if we have a complete event
            if buffer.contains("</event>") {
                // Extract just the event XML
                if let Some(start) = buffer.find("<event") {
                    if let Some(end) = buffer.find("</event>") {
                        let event_xml = buffer[start..=end + 7].to_string();
                        buffer.clear();
                        buffer.push_str(&event_xml);
                        return Ok(true);
                    }
                }
            }

            // Safety limit to prevent memory exhaustion
            if buffer.len() > 1024 * 1024 {
                buffer.clear();
                return Err(TakError::DecodingError("Event too large".into()));
            }
        }
    }

    /// Read a framed CoT event (TAK protocol with 0xBF magic)
    async fn read_framed_event(
        reader: &mut BufReader<OwnedReadHalf>,
        buffer: &mut String,
    ) -> Result<bool, TakError> {
        use tokio::io::AsyncReadExt;

        // Read magic byte
        let mut magic = [0u8; 1];
        match reader.read_exact(&mut magic).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
            Err(e) => return Err(TakError::IoError(e)),
        }

        if magic[0] != TAK_MAGIC {
            return Err(TakError::DecodingError(format!(
                "Invalid magic byte: expected 0x{:02X}, got 0x{:02X}",
                TAK_MAGIC, magic[0]
            )));
        }

        // Read version/type byte
        let mut version = [0u8; 1];
        reader
            .read_exact(&mut version)
            .await
            .map_err(TakError::IoError)?;

        if version[0] == 0x00 {
            // XML TCP: [0xBF][0x00][0xBF][payload]
            // Read second magic
            let mut magic2 = [0u8; 1];
            reader
                .read_exact(&mut magic2)
                .await
                .map_err(TakError::IoError)?;

            if magic2[0] != TAK_MAGIC {
                return Err(TakError::DecodingError("Invalid second magic byte".into()));
            }

            // Read until </event>
            return Self::read_raw_xml_event(reader, buffer).await;
        } else {
            // Protobuf: [0xBF][varint_length][payload]
            // The version byte is actually the first byte of the varint
            let length = Self::read_varint_with_first(reader, version[0]).await?;

            if length > 1024 * 1024 {
                return Err(TakError::DecodingError("Message too large".into()));
            }

            let mut payload = vec![0u8; length as usize];
            reader
                .read_exact(&mut payload)
                .await
                .map_err(TakError::IoError)?;

            // Try to parse as UTF-8 (could be XML or protobuf)
            match String::from_utf8(payload) {
                Ok(xml) => {
                    *buffer = xml;
                    Ok(true)
                }
                Err(_) => {
                    // TODO: Handle protobuf decoding
                    Err(TakError::DecodingError(
                        "Protobuf decoding not implemented".into(),
                    ))
                }
            }
        }
    }

    /// Read a varint, given the first byte already read
    async fn read_varint_with_first(
        reader: &mut BufReader<OwnedReadHalf>,
        first_byte: u8,
    ) -> Result<u64, TakError> {
        use tokio::io::AsyncReadExt;

        let mut value: u64 = (first_byte & 0x7F) as u64;
        let mut shift = 7;

        if first_byte & 0x80 == 0 {
            return Ok(value);
        }

        loop {
            let mut byte = [0u8; 1];
            reader
                .read_exact(&mut byte)
                .await
                .map_err(TakError::IoError)?;

            value |= ((byte[0] & 0x7F) as u64) << shift;

            if byte[0] & 0x80 == 0 {
                break;
            }

            shift += 7;
            if shift > 63 {
                return Err(TakError::DecodingError("Varint too large".into()));
            }
        }

        Ok(value)
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
    async fn drain_queue(&self, stream: &mut OwnedWriteHalf) -> Result<usize, TakError> {
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
        let stream = self.establish_connection().await?;

        // Split the stream into read and write halves
        let (read_half, mut write_half) = stream.into_split();

        // Send initial presence on write half
        self.send_presence_on_write(&mut write_half).await?;

        // Drain any queued messages
        self.drain_queue(&mut write_half).await?;

        // Spawn reader task for incoming events
        let reader_task = Self::spawn_reader_task(
            read_half,
            self.incoming_tx.clone(),
            self.config.protocol.version,
            self.metrics.clone(),
        );

        // Store write stream and reader task
        *self.write_stream.write().await = Some(write_half);
        *self.reader_task.write().await = Some(reader_task);
        self.connected.store(true, Ordering::SeqCst);
        self.metrics.record_connect();
        self.reconnect.write().unwrap().reset();

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), TakError> {
        info!("Disconnecting from TAK server");

        // Stop the reader task
        if let Some(task) = self.reader_task.write().await.take() {
            task.abort();
        }

        // Close the write stream
        let stream = { self.write_stream.write().await.take() };
        if let Some(mut stream) = stream {
            let _ = stream.shutdown().await;
        }

        self.connected.store(false, Ordering::SeqCst);
        self.metrics.record_disconnect();

        Ok(())
    }

    async fn send_cot(&self, event: &CotEvent, priority: Priority) -> Result<(), TakError> {
        if self.is_connected() {
            // Try to send directly - use async lock
            let mut guard = self.write_stream.write().await;
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

    async fn subscribe(&self, filter: CotFilter) -> Result<CotEventStream, TakError> {
        if !self.is_connected() {
            return Err(TakError::NotConnected);
        }

        // Create a new receiver from the broadcast channel
        let rx = self.incoming_tx.subscribe();

        // Return a stream that filters events based on the filter
        let stream = stream::unfold((rx, filter), move |(mut rx, filter)| async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        // Apply filter
                        if filter.matches(&event) {
                            return Some((Ok(event), (rx, filter)));
                        }
                        // Skip events that don't match filter
                        continue;
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("Subscriber lagged, missed {} events", count);
                        // Continue receiving
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // Channel closed, end stream
                        return None;
                    }
                }
            }
        });

        Ok(Box::pin(stream))
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
