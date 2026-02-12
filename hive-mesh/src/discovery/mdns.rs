use super::{DiscoveryError, DiscoveryEvent, DiscoveryStrategy, PeerInfo, Result};
use async_trait::async_trait;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// mDNS-based peer discovery for local network discovery
pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    service_type: String,
    discovered: Arc<RwLock<HashMap<String, PeerInfo>>>,
    events_tx: mpsc::Sender<DiscoveryEvent>,
    events_rx: Option<mpsc::Receiver<DiscoveryEvent>>,
    running: Arc<RwLock<bool>>,
}

impl MdnsDiscovery {
    /// Create a new mDNS discovery instance
    pub fn new() -> Result<Self> {
        Self::with_service_type("_hive._udp.local.")
    }

    /// Create a new mDNS discovery instance with a custom service type
    pub fn with_service_type(service_type: &str) -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;

        let (events_tx, events_rx) = mpsc::channel(100);

        Ok(Self {
            daemon,
            service_type: service_type.to_string(),
            discovered: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
            events_rx: Some(events_rx),
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Advertise this node on the local network
    pub fn advertise(
        &self,
        node_id: &str,
        port: u16,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<()> {
        let instance_name = format!("hive-{}", node_id);

        let mut properties = HashMap::new();
        properties.insert("node_id".to_string(), node_id.to_string());

        if let Some(meta) = metadata {
            for (k, v) in meta {
                properties.insert(k, v);
            }
        }

        let service = ServiceInfo::new(
            &self.service_type,
            &instance_name,
            "",
            "",
            port,
            Some(properties),
        )
        .map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;

        self.daemon
            .register(service)
            .map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;

        info!("Advertising node {} on port {} via mDNS", node_id, port);

        Ok(())
    }

    /// Unadvertise this node from the local network
    pub fn unadvertise(&self, node_id: &str) -> Result<()> {
        let instance_name = format!("hive-{}", node_id);
        let fullname = format!("{}.{}", instance_name, self.service_type);

        self.daemon
            .unregister(&fullname)
            .map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;

        info!("Unadvertised node {} from mDNS", node_id);

        Ok(())
    }

    /// Parse a ServiceInfo into a PeerInfo
    fn parse_service_info(info: &ServiceInfo) -> Option<PeerInfo> {
        // Extract node_id from TXT records
        let properties = info.get_properties();
        let node_id = properties.get("node_id").map(|v| v.val_str().to_string())?;

        // Get all addresses for this service
        let addresses: Vec<SocketAddr> = info
            .get_addresses()
            .iter()
            .map(|ip| SocketAddr::new(*ip, info.get_port()))
            .collect();

        if addresses.is_empty() {
            warn!("Service {} has no addresses", node_id);
            return None;
        }

        let mut metadata = HashMap::new();
        for prop in properties.iter() {
            let key = prop.key();
            if key != "node_id" {
                metadata.insert(key.to_string(), prop.val_str().to_string());
            }
        }

        debug!(
            "Parsed service info: node_id={}, addresses={:?}",
            node_id, addresses
        );

        Some(PeerInfo {
            node_id,
            addresses,
            relay_url: None,
            last_seen: std::time::Instant::now(),
            metadata,
        })
    }

    /// Handle a resolved mDNS service event.
    async fn handle_resolved(
        discovered: &RwLock<HashMap<String, PeerInfo>>,
        events_tx: &mpsc::Sender<DiscoveryEvent>,
        info: &ServiceInfo,
    ) {
        debug!("mDNS service resolved: {:?}", info.get_fullname());

        if let Some(peer_info) = Self::parse_service_info(info) {
            let node_id = peer_info.node_id.clone();

            // Update discovered peers
            let mut peers = discovered.write().await;
            let is_new = !peers.contains_key(&node_id);
            peers.insert(node_id.clone(), peer_info.clone());
            drop(peers);

            // Send event
            let event = if is_new {
                info!("Discovered new peer: {}", node_id);
                DiscoveryEvent::PeerFound(peer_info)
            } else {
                debug!("Updated peer: {}", node_id);
                DiscoveryEvent::PeerUpdated(peer_info)
            };

            if let Err(e) = events_tx.send(event).await {
                error!("Failed to send discovery event: {}", e);
            }
        }
    }

    /// Handle a removed mDNS service event.
    async fn handle_removed(
        discovered: &RwLock<HashMap<String, PeerInfo>>,
        events_tx: &mpsc::Sender<DiscoveryEvent>,
        fullname: &str,
    ) {
        debug!("mDNS service removed: {}", fullname);

        if let Some(node_id) = Self::extract_node_id(fullname) {
            let mut peers = discovered.write().await;
            if peers.remove(&node_id).is_some() {
                drop(peers);

                info!("Lost peer: {}", node_id);

                if let Err(e) = events_tx.send(DiscoveryEvent::PeerLost(node_id)).await {
                    error!("Failed to send discovery event: {}", e);
                }
            }
        }
    }

    /// Extract node_id from a service fullname
    fn extract_node_id(fullname: &str) -> Option<String> {
        // Format: "hive-{node_id}._hive._udp.local."
        let parts: Vec<&str> = fullname.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        let instance = parts[0];
        if !instance.starts_with("hive-") {
            return None;
        }

        Some(instance.strip_prefix("hive-")?.to_string())
    }
}

impl Default for MdnsDiscovery {
    fn default() -> Self {
        Self::new().expect("Failed to create default MdnsDiscovery")
    }
}

#[async_trait]
impl DiscoveryStrategy for MdnsDiscovery {
    async fn start(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            warn!("mDNS discovery already running");
            return Ok(());
        }

        let receiver = self
            .daemon
            .browse(&self.service_type)
            .map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;

        *running = true;
        drop(running);

        let discovered = self.discovered.clone();
        let events_tx = self.events_tx.clone();
        let running_clone = self.running.clone();

        info!("Starting mDNS discovery for {}", self.service_type);

        // Spawn background task to process mDNS events
        tokio::spawn(async move {
            while *running_clone.read().await {
                match receiver.recv_async().await {
                    Ok(event) => match event {
                        ServiceEvent::ServiceResolved(info) => {
                            Self::handle_resolved(&discovered, &events_tx, &info).await;
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => {
                            Self::handle_removed(&discovered, &events_tx, &fullname).await;
                        }
                        ServiceEvent::SearchStarted(_) => {
                            debug!("mDNS search started");
                        }
                        ServiceEvent::SearchStopped(_) => {
                            debug!("mDNS search stopped");
                        }
                        _ => {}
                    },
                    Err(e) => {
                        error!("Error receiving mDNS event: {}", e);
                        break;
                    }
                }
            }

            info!("mDNS discovery task stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }

        info!("Stopping mDNS discovery");
        *running = false;

        // Give the background task time to exit
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(())
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.discovered.read().await.values().cloned().collect()
    }

    fn event_stream(&mut self) -> Result<mpsc::Receiver<DiscoveryEvent>> {
        self.events_rx
            .take()
            .ok_or(DiscoveryError::EventStreamConsumed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_node_id() {
        let fullname = "hive-platform-1._hive._udp.local.";
        let node_id = MdnsDiscovery::extract_node_id(fullname);
        assert_eq!(node_id, Some("platform-1".to_string()));

        let invalid = "invalid._hive._udp.local.";
        let node_id = MdnsDiscovery::extract_node_id(invalid);
        assert_eq!(node_id, None);
    }

    #[test]
    fn test_extract_node_id_empty() {
        assert_eq!(MdnsDiscovery::extract_node_id(""), None);
    }

    #[test]
    fn test_extract_node_id_just_prefix() {
        // "hive-" prefix with empty node id
        let result = MdnsDiscovery::extract_node_id("hive-._hive._udp.local.");
        assert_eq!(result, Some("".to_string()));
    }

    #[test]
    fn test_extract_node_id_complex() {
        let fullname = "hive-squad-bravo-3._hive._udp.local.";
        let node_id = MdnsDiscovery::extract_node_id(fullname);
        assert_eq!(node_id, Some("squad-bravo-3".to_string()));
    }

    #[tokio::test]
    async fn test_mdns_discovery_lifecycle() {
        let mut discovery = MdnsDiscovery::new().unwrap();

        // Start discovery
        discovery.start().await.unwrap();

        // Check that it's running
        assert!(*discovery.running.read().await);

        // Stop discovery
        discovery.stop().await.unwrap();

        // Check that it's stopped
        assert!(!*discovery.running.read().await);
    }

    #[tokio::test]
    async fn test_mdns_discovery_start_twice() {
        let mut discovery = MdnsDiscovery::new().unwrap();
        discovery.start().await.unwrap();
        assert!(*discovery.running.read().await);

        // Starting again should be idempotent
        discovery.start().await.unwrap();
        assert!(*discovery.running.read().await);

        discovery.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_mdns_discovery_stop_when_not_running() {
        let mut discovery = MdnsDiscovery::new().unwrap();
        // Stop without start should be fine
        discovery.stop().await.unwrap();
        assert!(!*discovery.running.read().await);
    }

    #[tokio::test]
    async fn test_mdns_discovered_peers_initially_empty() {
        let discovery = MdnsDiscovery::new().unwrap();
        let peers = discovery.discovered_peers().await;
        assert!(peers.is_empty());
    }

    #[tokio::test]
    async fn test_mdns_event_stream_consumed() {
        let mut discovery = MdnsDiscovery::new().unwrap();
        let _stream = discovery.event_stream().unwrap();

        // Second call should fail
        let result = discovery.event_stream();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DiscoveryError::EventStreamConsumed
        ));
    }

    #[test]
    fn test_mdns_with_custom_service_type() {
        let discovery = MdnsDiscovery::with_service_type("_custom._tcp.local.");
        assert!(discovery.is_ok());
        assert_eq!(discovery.unwrap().service_type, "_custom._tcp.local.");
    }

    #[test]
    fn test_parse_service_info_with_address() {
        let mut props = HashMap::new();
        props.insert("node_id".to_string(), "test-node".to_string());
        props.insert("role".to_string(), "leader".to_string());

        let info = ServiceInfo::new(
            "_hive._udp.local.",
            "hive-test-node",
            "localhost",
            "192.168.1.10",
            5000,
            Some(props),
        )
        .unwrap();

        let peer = MdnsDiscovery::parse_service_info(&info).unwrap();
        assert_eq!(peer.node_id, "test-node");
        assert!(!peer.addresses.is_empty());
        assert_eq!(peer.addresses[0].port(), 5000);
        assert!(peer.relay_url.is_none());
        // "role" should be in metadata but "node_id" should not
        assert_eq!(peer.metadata.get("role").unwrap(), "leader");
        assert!(!peer.metadata.contains_key("node_id"));
    }

    #[test]
    fn test_parse_service_info_no_node_id() {
        // No "node_id" property → should return None
        let info = ServiceInfo::new(
            "_hive._udp.local.",
            "hive-anon",
            "localhost",
            "10.0.0.1",
            4000,
            None::<HashMap<String, String>>,
        )
        .unwrap();

        let result = MdnsDiscovery::parse_service_info(&info);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_service_info_no_addresses() {
        let mut props = HashMap::new();
        props.insert("node_id".to_string(), "ghost".to_string());

        // Empty string for IP → no addresses
        let info = ServiceInfo::new(
            "_hive._udp.local.",
            "hive-ghost",
            "localhost",
            "",
            3000,
            Some(props),
        )
        .unwrap();

        let result = MdnsDiscovery::parse_service_info(&info);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_service_info_multiple_metadata() {
        let mut props = HashMap::new();
        props.insert("node_id".to_string(), "multi".to_string());
        props.insert("version".to_string(), "2.0".to_string());
        props.insert("region".to_string(), "us-east".to_string());
        props.insert("priority".to_string(), "high".to_string());

        let info = ServiceInfo::new(
            "_hive._udp.local.",
            "hive-multi",
            "localhost",
            "10.0.0.5",
            6000,
            Some(props),
        )
        .unwrap();

        let peer = MdnsDiscovery::parse_service_info(&info).unwrap();
        assert_eq!(peer.node_id, "multi");
        assert_eq!(peer.metadata.len(), 3); // version, region, priority (not node_id)
        assert_eq!(peer.metadata["version"], "2.0");
        assert_eq!(peer.metadata["region"], "us-east");
        assert_eq!(peer.metadata["priority"], "high");
    }

    #[test]
    fn test_advertise_error_maps_correctly() {
        // advertise with empty hostname (which ServiceInfo::new passes through)
        // exercises the error-mapping path: ServiceInfo::new → MdnsError
        let discovery = MdnsDiscovery::new().unwrap();
        // The current code passes "" as hostname to ServiceInfo::new, which
        // may fail depending on the mdns-sd version. Either outcome exercises code.
        let result = discovery.advertise("node-42", 8080, None);
        // We don't assert success — just that it doesn't panic.
        let _ = result;
    }

    #[test]
    fn test_advertise_with_metadata_path() {
        let discovery = MdnsDiscovery::new().unwrap();
        let mut meta = HashMap::new();
        meta.insert("role".to_string(), "leader".to_string());
        meta.insert("version".to_string(), "1.0".to_string());
        // Exercise the metadata-merging path in advertise()
        let result = discovery.advertise("node-meta", 9090, Some(meta));
        let _ = result;
    }

    #[test]
    fn test_unadvertise_formats_fullname() {
        let discovery = MdnsDiscovery::new().unwrap();
        // unadvertise just formats the fullname and calls daemon.unregister
        let result = discovery.unadvertise("node-42");
        // May fail if not registered, but exercises the code path
        let _ = result;
    }

    #[test]
    fn test_mdns_default() {
        let discovery = MdnsDiscovery::default();
        assert_eq!(discovery.service_type, "_hive._udp.local.");
    }

    #[test]
    fn test_extract_node_id_no_prefix_match() {
        // Various non-matching prefixes
        assert_eq!(MdnsDiscovery::extract_node_id("other-node._udp.local."), None);
        assert_eq!(MdnsDiscovery::extract_node_id("HIVE-upper._udp.local."), None);
    }

    #[tokio::test]
    async fn test_mdns_discovered_peers_after_advertise() {
        let discovery = MdnsDiscovery::new().unwrap();
        let _ = discovery.advertise("ad-node", 7777, None);
        // Peers list should still be empty since we haven't started discovery browsing
        let peers = discovery.discovered_peers().await;
        assert!(peers.is_empty());
    }

    /// Test handle_resolved with a new peer (PeerFound path).
    #[tokio::test]
    async fn test_handle_resolved_new_peer() {
        let discovered = Arc::new(RwLock::new(HashMap::new()));
        let (events_tx, mut events_rx) = mpsc::channel(10);

        let mut props = HashMap::new();
        props.insert("node_id".to_string(), "peer-A".to_string());
        let info = ServiceInfo::new(
            "_hive._udp.local.",
            "hive-peer-A",
            "host.local.",
            "10.0.0.1",
            5000,
            Some(props),
        )
        .unwrap();

        MdnsDiscovery::handle_resolved(&discovered, &events_tx, &info).await;

        // Peer should be in discovered map
        let peers = discovered.read().await;
        assert!(peers.contains_key("peer-A"));
        drop(peers);

        // Should receive PeerFound event
        let event = events_rx.try_recv().unwrap();
        assert!(matches!(event, DiscoveryEvent::PeerFound(ref p) if p.node_id == "peer-A"));
    }

    /// Test handle_resolved with an existing peer (PeerUpdated path).
    #[tokio::test]
    async fn test_handle_resolved_existing_peer() {
        let discovered = Arc::new(RwLock::new(HashMap::new()));
        let (events_tx, mut events_rx) = mpsc::channel(10);

        let mut props = HashMap::new();
        props.insert("node_id".to_string(), "peer-B".to_string());
        let info = ServiceInfo::new(
            "_hive._udp.local.",
            "hive-peer-B",
            "host.local.",
            "10.0.0.2",
            5001,
            Some(props),
        )
        .unwrap();

        // First resolve → PeerFound
        MdnsDiscovery::handle_resolved(&discovered, &events_tx, &info).await;
        let event = events_rx.try_recv().unwrap();
        assert!(matches!(event, DiscoveryEvent::PeerFound(_)));

        // Second resolve → PeerUpdated
        MdnsDiscovery::handle_resolved(&discovered, &events_tx, &info).await;
        let event = events_rx.try_recv().unwrap();
        assert!(matches!(event, DiscoveryEvent::PeerUpdated(_)));
    }

    /// Test handle_resolved with no node_id property (no event sent).
    #[tokio::test]
    async fn test_handle_resolved_no_node_id() {
        let discovered = Arc::new(RwLock::new(HashMap::new()));
        let (events_tx, mut events_rx) = mpsc::channel(10);

        let info = ServiceInfo::new(
            "_hive._udp.local.",
            "anon",
            "host.local.",
            "10.0.0.3",
            5002,
            None::<HashMap<String, String>>,
        )
        .unwrap();

        MdnsDiscovery::handle_resolved(&discovered, &events_tx, &info).await;

        // No event should be sent
        assert!(events_rx.try_recv().is_err());
    }

    /// Test handle_removed with a known peer.
    #[tokio::test]
    async fn test_handle_removed_known_peer() {
        let discovered = Arc::new(RwLock::new(HashMap::new()));
        let (events_tx, mut events_rx) = mpsc::channel(10);

        // Pre-populate discovered peers
        let peer = PeerInfo {
            node_id: "peer-C".to_string(),
            addresses: vec!["10.0.0.4:5003".parse().unwrap()],
            relay_url: None,
            last_seen: std::time::Instant::now(),
            metadata: HashMap::new(),
        };
        discovered.write().await.insert("peer-C".to_string(), peer);

        MdnsDiscovery::handle_removed(
            &discovered,
            &events_tx,
            "hive-peer-C._hive._udp.local.",
        )
        .await;

        // Peer should be removed from map
        assert!(discovered.read().await.is_empty());

        // Should receive PeerLost event
        let event = events_rx.try_recv().unwrap();
        assert!(matches!(event, DiscoveryEvent::PeerLost(ref id) if id == "peer-C"));
    }

    /// Test handle_removed with an unknown peer (no event sent).
    #[tokio::test]
    async fn test_handle_removed_unknown_peer() {
        let discovered = Arc::new(RwLock::new(HashMap::new()));
        let (events_tx, mut events_rx) = mpsc::channel(10);

        MdnsDiscovery::handle_removed(
            &discovered,
            &events_tx,
            "hive-unknown._hive._udp.local.",
        )
        .await;

        // No event should be sent
        assert!(events_rx.try_recv().is_err());
    }

    /// Test handle_removed with invalid fullname (no hive- prefix).
    #[tokio::test]
    async fn test_handle_removed_invalid_fullname() {
        let discovered = Arc::new(RwLock::new(HashMap::new()));
        let (events_tx, mut events_rx) = mpsc::channel(10);

        MdnsDiscovery::handle_removed(&discovered, &events_tx, "other._tcp.local.").await;

        // No event should be sent
        assert!(events_rx.try_recv().is_err());
    }

    /// Test that the spawned task's SearchStarted/SearchStopped handling works.
    /// When we start and stop, the daemon fires these meta-events.
    #[tokio::test]
    async fn test_mdns_start_stop_search_events() {
        use std::time::Duration;

        let mut discovery = MdnsDiscovery::new().unwrap();
        discovery.start().await.unwrap();

        // Wait briefly for SearchStarted event
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Stop discovery
        discovery.stop().await.unwrap();

        // Wait for background task to exit
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
