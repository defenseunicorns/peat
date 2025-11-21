use crate::{DiscoveryError, DiscoveryEvent, DiscoveryStrategy, PeerInfo, Result};
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
                            debug!("mDNS service resolved: {:?}", info.get_fullname());

                            if let Some(peer_info) = Self::parse_service_info(&info) {
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
                        ServiceEvent::ServiceRemoved(_, fullname) => {
                            debug!("mDNS service removed: {}", fullname);

                            if let Some(node_id) = Self::extract_node_id(&fullname) {
                                let mut peers = discovered.write().await;
                                if peers.remove(&node_id).is_some() {
                                    drop(peers);

                                    info!("Lost peer: {}", node_id);

                                    if let Err(e) =
                                        events_tx.send(DiscoveryEvent::PeerLost(node_id)).await
                                    {
                                        error!("Failed to send discovery event: {}", e);
                                    }
                                }
                            }
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
}
