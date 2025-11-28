//! Mesh SA UDP multicast transport implementation

use async_trait::async_trait;
use hive_protocol::cot::CotEvent;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio::net::UdpSocket;
use tracing::{debug, error, info};

use super::config::{TakProtocolVersion, TakTransportConfig, TakTransportMode};
use super::error::TakError;
use super::metrics::{QueueDepthMetrics, TakMetrics};
use super::queue::TakMessageQueue;
use super::traits::{CotEventStream, CotFilter, Priority, TakTransport};

/// TAK Protocol magic byte
const TAK_MAGIC: u8 = 0xBF;

/// Default Mesh SA port
pub const DEFAULT_MESH_SA_PORT: u16 = 6969;

/// Default Mesh SA multicast group
pub const DEFAULT_MESH_SA_GROUP: &str = "239.2.3.1";

/// Mesh SA UDP multicast transport
pub struct MeshSaTransport {
    config: TakTransportConfig,
    multicast_group: IpAddr,
    port: u16,
    #[allow(dead_code)] // Used for interface binding in future
    interface: Option<String>,
    socket: RwLock<Option<Arc<UdpSocket>>>,
    connected: AtomicBool,
    queue: RwLock<TakMessageQueue>,
    metrics: Arc<TakMetrics>,
}

impl MeshSaTransport {
    /// Create a new Mesh SA transport
    pub fn new(config: TakTransportConfig) -> Result<Self, TakError> {
        let (multicast_group, port, interface) = match &config.mode {
            TakTransportMode::MeshSa {
                multicast_group,
                port,
                interface,
            } => (*multicast_group, *port, interface.clone()),
            TakTransportMode::Hybrid {
                mesh_group,
                mesh_port,
                ..
            } => (*mesh_group, *mesh_port, None),
            _ => {
                return Err(TakError::InvalidConfig(
                    "MeshSaTransport requires MeshSa or Hybrid mode".into(),
                ))
            }
        };

        let queue = TakMessageQueue::new(config.queue.clone());

        Ok(Self {
            config,
            multicast_group,
            port,
            interface,
            socket: RwLock::new(None),
            connected: AtomicBool::new(false),
            queue: RwLock::new(queue),
            metrics: Arc::new(TakMetrics::new()),
        })
    }

    /// Create a multicast socket
    fn create_multicast_socket(&self) -> Result<Socket, TakError> {
        let domain = match self.multicast_group {
            IpAddr::V4(_) => Domain::IPV4,
            IpAddr::V6(_) => Domain::IPV6,
        };

        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| TakError::MulticastError(format!("Failed to create socket: {}", e)))?;

        // Allow address reuse
        socket
            .set_reuse_address(true)
            .map_err(|e| TakError::MulticastError(format!("Failed to set reuse address: {}", e)))?;

        #[cfg(not(windows))]
        socket
            .set_reuse_port(true)
            .map_err(|e| TakError::MulticastError(format!("Failed to set reuse port: {}", e)))?;

        // Bind to the port
        let bind_addr: SocketAddr = match self.multicast_group {
            IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), self.port),
            IpAddr::V6(_) => {
                SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), self.port)
            }
        };

        socket
            .bind(&bind_addr.into())
            .map_err(|e| TakError::MulticastError(format!("Failed to bind: {}", e)))?;

        // Join multicast group
        match self.multicast_group {
            IpAddr::V4(addr) => {
                socket
                    .join_multicast_v4(&addr, &Ipv4Addr::UNSPECIFIED)
                    .map_err(|e| {
                        TakError::MulticastError(format!("Failed to join multicast group: {}", e))
                    })?;
            }
            IpAddr::V6(addr) => {
                socket.join_multicast_v6(&addr, 0).map_err(|e| {
                    TakError::MulticastError(format!("Failed to join multicast group: {}", e))
                })?;
            }
        }

        // Set non-blocking
        socket
            .set_nonblocking(true)
            .map_err(|e| TakError::MulticastError(format!("Failed to set non-blocking: {}", e)))?;

        Ok(socket)
    }

    /// Send a CoT event via multicast
    async fn send_event_multicast(
        &self,
        socket: &UdpSocket,
        event: &CotEvent,
    ) -> Result<(), TakError> {
        let xml = event
            .to_xml()
            .map_err(|e| TakError::EncodingError(format!("XML encoding failed: {}", e)))?;
        let payload = xml.as_bytes();

        // Frame the message for Mesh SA
        let frame = self.frame_mesh_sa(payload);

        let dest = SocketAddr::new(self.multicast_group, self.port);
        socket
            .send_to(&frame, dest)
            .await
            .map_err(TakError::IoError)?;

        self.metrics.record_send(frame.len());
        debug!(
            "Sent CoT event via multicast: {} ({} bytes)",
            event.uid,
            frame.len()
        );

        Ok(())
    }

    /// Frame message for TAK Mesh SA protocol
    ///
    /// Format: [0xBF][version][0xBF][payload] for XML
    /// Format: [0xBF][0x01][0xBF][varint_len][payload] for Protobuf
    fn frame_mesh_sa(&self, payload: &[u8]) -> Vec<u8> {
        match self.config.protocol.version {
            TakProtocolVersion::XmlTcp => {
                // XML framing
                let mut frame = Vec::with_capacity(3 + payload.len());
                frame.push(TAK_MAGIC);
                frame.push(0x00); // Version 0 = XML
                frame.push(TAK_MAGIC);
                frame.extend_from_slice(payload);
                frame
            }
            TakProtocolVersion::ProtobufV1 => {
                // Protobuf framing (using XML payload for now)
                let mut frame = Vec::with_capacity(4 + payload.len());
                frame.push(TAK_MAGIC);
                frame.push(0x01); // Version 1 = Protobuf
                frame.push(TAK_MAGIC);
                Self::encode_varint(payload.len() as u64, &mut frame);
                frame.extend_from_slice(payload);
                frame
            }
        }
    }

    /// Encode a value as a varint
    fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
        while value >= 0x80 {
            buf.push((value as u8 & 0x7F) | 0x80);
            value >>= 7;
        }
        buf.push(value as u8);
    }
}

#[async_trait]
impl TakTransport for MeshSaTransport {
    async fn connect(&mut self) -> Result<(), TakError> {
        info!(
            "Joining Mesh SA multicast group {}:{}",
            self.multicast_group, self.port
        );

        let std_socket = self.create_multicast_socket()?;
        let socket = UdpSocket::from_std(std_socket.into()).map_err(|e| {
            TakError::MulticastError(format!("Failed to create async socket: {}", e))
        })?;

        *self.socket.write().unwrap() = Some(Arc::new(socket));
        self.connected.store(true, Ordering::SeqCst);
        self.metrics.record_connect();

        info!("Joined Mesh SA multicast group");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), TakError> {
        info!("Leaving Mesh SA multicast group");

        // Drop the socket
        *self.socket.write().unwrap() = None;
        self.connected.store(false, Ordering::SeqCst);
        self.metrics.record_disconnect();

        Ok(())
    }

    async fn send_cot(&self, event: &CotEvent, priority: Priority) -> Result<(), TakError> {
        let socket = {
            let guard = self.socket.read().unwrap();
            guard.clone()
        };

        if let Some(socket) = socket {
            match self.send_event_multicast(&socket, event).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    error!("Multicast send failed: {}", e);
                    self.metrics.record_error(&e.to_string());
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
        // TODO: Implement subscription with background receiver task
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
    fn test_new_mesh_transport() {
        let config = TakTransportConfig {
            mode: TakTransportMode::MeshSa {
                multicast_group: DEFAULT_MESH_SA_GROUP.parse().unwrap(),
                port: DEFAULT_MESH_SA_PORT,
                interface: None,
            },
            ..Default::default()
        };

        let transport = MeshSaTransport::new(config);
        assert!(transport.is_ok());
    }

    #[test]
    fn test_new_mesh_transport_wrong_mode() {
        let config = TakTransportConfig {
            mode: TakTransportMode::TakServer {
                address: "127.0.0.1:8087".parse().unwrap(),
                use_tls: false,
            },
            ..Default::default()
        };

        let transport = MeshSaTransport::new(config);
        assert!(transport.is_err());
    }

    #[test]
    fn test_frame_mesh_sa_xml() {
        let config = TakTransportConfig {
            mode: TakTransportMode::MeshSa {
                multicast_group: DEFAULT_MESH_SA_GROUP.parse().unwrap(),
                port: DEFAULT_MESH_SA_PORT,
                interface: None,
            },
            ..Default::default()
        };
        let mut config = config;
        config.protocol.version = TakProtocolVersion::XmlTcp;

        let transport = MeshSaTransport::new(config).unwrap();
        let payload = b"<event/>";
        let frame = transport.frame_mesh_sa(payload);

        assert_eq!(frame[0], TAK_MAGIC);
        assert_eq!(frame[1], 0x00); // XML version
        assert_eq!(frame[2], TAK_MAGIC);
        assert_eq!(&frame[3..], payload);
    }

    #[test]
    fn test_varint_encoding() {
        let mut buf = Vec::new();
        MeshSaTransport::encode_varint(300, &mut buf);
        assert_eq!(buf, vec![0xAC, 0x02]);
    }
}
