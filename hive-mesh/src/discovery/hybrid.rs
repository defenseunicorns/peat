use super::{DiscoveryError, DiscoveryEvent, DiscoveryStrategy, PeerInfo, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// Combines multiple discovery strategies into a unified discovery system
///
/// HybridDiscovery allows combining different discovery mechanisms (mDNS, static config,
/// relay-based, etc.) to discover peers through multiple channels simultaneously.
///
/// Example usage:
/// ```no_run
/// use hive_mesh::discovery::{HybridDiscovery, MdnsDiscovery, StaticDiscovery};
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut discovery = HybridDiscovery::new();
///
/// // Add mDNS for local network discovery
/// let mdns = MdnsDiscovery::new()?;
/// discovery.add_strategy("mdns", Box::new(mdns));
///
/// // Add static config for pre-configured peers
/// let static_disc = StaticDiscovery::from_file(Path::new("config/peers.toml"))?;
/// discovery.add_strategy("static", Box::new(static_disc));
///
/// // Start all strategies
/// discovery.start_all().await?;
///
/// // Get all discovered peers from all sources
/// let peers = discovery.all_discovered_peers().await;
/// # Ok(())
/// # }
/// ```
pub struct HybridDiscovery {
    strategies: HashMap<String, Box<dyn DiscoveryStrategy>>,
    combined_peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    events_tx: mpsc::Sender<DiscoveryEvent>,
    events_rx: Option<mpsc::Receiver<DiscoveryEvent>>,
    started: Arc<RwLock<bool>>,
}

impl HybridDiscovery {
    /// Create a new hybrid discovery manager
    pub fn new() -> Self {
        let (events_tx, events_rx) = mpsc::channel(1000);

        Self {
            strategies: HashMap::new(),
            combined_peers: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
            events_rx: Some(events_rx),
            started: Arc::new(RwLock::new(false)),
        }
    }

    /// Add a discovery strategy with a name
    pub fn add_strategy(&mut self, name: impl Into<String>, strategy: Box<dyn DiscoveryStrategy>) {
        let name = name.into();
        info!("Adding discovery strategy: {}", name);
        self.strategies.insert(name, strategy);
    }

    /// Remove a discovery strategy by name
    pub fn remove_strategy(&mut self, name: &str) -> Option<Box<dyn DiscoveryStrategy>> {
        info!("Removing discovery strategy: {}", name);
        self.strategies.remove(name)
    }

    /// Start all discovery strategies
    pub async fn start_all(&mut self) -> Result<()> {
        let mut started = self.started.write().await;
        if *started {
            warn!("Hybrid discovery already started");
            return Ok(());
        }

        info!(
            "Starting hybrid discovery with {} strategies",
            self.strategies.len()
        );

        // Start each strategy and subscribe to its events
        for (name, strategy) in self.strategies.iter_mut() {
            info!("Starting discovery strategy: {}", name);

            strategy.start().await.map_err(|e| {
                DiscoveryError::ConfigError(format!("Failed to start strategy '{}': {}", name, e))
            })?;

            // Get the event stream
            let mut events = strategy.event_stream().map_err(|e| {
                DiscoveryError::ConfigError(format!(
                    "Failed to get event stream for strategy '{}': {}",
                    name, e
                ))
            })?;

            let combined_peers = self.combined_peers.clone();
            let events_tx = self.events_tx.clone();
            let strategy_name = name.clone();

            // Spawn task to forward events from this strategy
            tokio::spawn(async move {
                while let Some(event) = events.recv().await {
                    debug!("Event from strategy '{}': {:?}", strategy_name, event);

                    match &event {
                        DiscoveryEvent::PeerFound(peer_info) => {
                            let mut peers = combined_peers.write().await;
                            let node_id = peer_info.node_id.clone();
                            let is_new = !peers.contains_key(&node_id);

                            peers.insert(node_id.clone(), peer_info.clone());
                            drop(peers);

                            // Forward appropriate event
                            let forward_event = if is_new {
                                info!("New peer discovered via '{}': {}", strategy_name, node_id);
                                DiscoveryEvent::PeerFound(peer_info.clone())
                            } else {
                                debug!("Updated peer via '{}': {}", strategy_name, node_id);
                                DiscoveryEvent::PeerUpdated(peer_info.clone())
                            };

                            if let Err(e) = events_tx.send(forward_event).await {
                                warn!("Failed to forward discovery event: {}", e);
                            }
                        }
                        DiscoveryEvent::PeerLost(node_id) => {
                            let mut peers = combined_peers.write().await;
                            if peers.remove(node_id).is_some() {
                                drop(peers);

                                info!("Peer lost via '{}': {}", strategy_name, node_id);

                                if let Err(e) = events_tx
                                    .send(DiscoveryEvent::PeerLost(node_id.clone()))
                                    .await
                                {
                                    warn!("Failed to forward peer lost event: {}", e);
                                }
                            }
                        }
                        DiscoveryEvent::PeerUpdated(peer_info) => {
                            let mut peers = combined_peers.write().await;
                            let node_id = peer_info.node_id.clone();

                            if peers.contains_key(&node_id) {
                                peers.insert(node_id.clone(), peer_info.clone());
                                drop(peers);

                                debug!("Peer updated via '{}': {}", strategy_name, node_id);

                                if let Err(e) = events_tx
                                    .send(DiscoveryEvent::PeerUpdated(peer_info.clone()))
                                    .await
                                {
                                    warn!("Failed to forward peer updated event: {}", e);
                                }
                            }
                        }
                    }
                }

                info!("Event forwarding task for '{}' stopped", strategy_name);
            });
        }

        *started = true;
        info!("Hybrid discovery started successfully");

        Ok(())
    }

    /// Stop all discovery strategies
    pub async fn stop_all(&mut self) -> Result<()> {
        let mut started = self.started.write().await;
        if !*started {
            return Ok(());
        }

        info!("Stopping hybrid discovery");

        for (name, strategy) in self.strategies.iter_mut() {
            info!("Stopping discovery strategy: {}", name);
            if let Err(e) = strategy.stop().await {
                warn!("Error stopping strategy '{}': {}", name, e);
            }
        }

        *started = false;

        // Give background tasks time to exit
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        info!("Hybrid discovery stopped");

        Ok(())
    }

    /// Get all discovered peers from all strategies
    pub async fn all_discovered_peers(&self) -> Vec<PeerInfo> {
        self.combined_peers.read().await.values().cloned().collect()
    }

    /// Get the number of active strategies
    pub fn strategy_count(&self) -> usize {
        self.strategies.len()
    }

    /// Check if a specific strategy is registered
    pub fn has_strategy(&self, name: &str) -> bool {
        self.strategies.contains_key(name)
    }
}

impl Default for HybridDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DiscoveryStrategy for HybridDiscovery {
    async fn start(&mut self) -> Result<()> {
        self.start_all().await
    }

    async fn stop(&mut self) -> Result<()> {
        self.stop_all().await
    }

    async fn discovered_peers(&self) -> Vec<PeerInfo> {
        self.all_discovered_peers().await
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
    use crate::discovery::static_config::{DiscoveryConfig, StaticPeerConfig};
    use crate::discovery::StaticDiscovery;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_hybrid_discovery_basic() {
        let mut hybrid = HybridDiscovery::new();

        // Create a simple static discovery
        let config = DiscoveryConfig {
            peers: vec![StaticPeerConfig {
                node_id: "test-peer".to_string(),
                addresses: vec!["10.0.0.1:5000".to_string()],
                relay_url: None,
                priority: 128,
                metadata: HashMap::new(),
            }],
        };

        let static_disc = StaticDiscovery::from_config(config).unwrap();
        hybrid.add_strategy("static", Box::new(static_disc));

        assert_eq!(hybrid.strategy_count(), 1);
        assert!(hybrid.has_strategy("static"));

        // Get event stream before starting
        let mut events = hybrid.event_stream().unwrap();

        // Start discovery
        hybrid.start_all().await.unwrap();

        // Wait a bit for events to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should discover the peer
        let peers = hybrid.all_discovered_peers().await;
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].node_id, "test-peer");

        // Check event
        let event = events.try_recv().unwrap();
        assert!(matches!(event, DiscoveryEvent::PeerFound(_)));

        // Stop discovery
        hybrid.stop_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_hybrid_discovery_multiple_strategies() {
        let mut hybrid = HybridDiscovery::new();

        // Add two static discovery strategies with different peers
        let config1 = DiscoveryConfig {
            peers: vec![StaticPeerConfig {
                node_id: "peer-1".to_string(),
                addresses: vec!["10.0.0.1:5000".to_string()],
                relay_url: None,
                priority: 128,
                metadata: HashMap::new(),
            }],
        };

        let config2 = DiscoveryConfig {
            peers: vec![StaticPeerConfig {
                node_id: "peer-2".to_string(),
                addresses: vec!["10.0.0.2:5000".to_string()],
                relay_url: None,
                priority: 128,
                metadata: HashMap::new(),
            }],
        };

        hybrid.add_strategy(
            "static-1",
            Box::new(StaticDiscovery::from_config(config1).unwrap()),
        );
        hybrid.add_strategy(
            "static-2",
            Box::new(StaticDiscovery::from_config(config2).unwrap()),
        );

        assert_eq!(hybrid.strategy_count(), 2);

        // Start discovery
        hybrid.start_all().await.unwrap();

        // Wait for events to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should discover both peers
        let peers = hybrid.all_discovered_peers().await;
        assert_eq!(peers.len(), 2);

        let peer_ids: Vec<&str> = peers.iter().map(|p| p.node_id.as_str()).collect();
        assert!(peer_ids.contains(&"peer-1"));
        assert!(peer_ids.contains(&"peer-2"));

        // Stop discovery
        hybrid.stop_all().await.unwrap();
    }

    #[test]
    fn test_strategy_management() {
        let mut hybrid = HybridDiscovery::new();

        let config = DiscoveryConfig { peers: vec![] };
        let static_disc = StaticDiscovery::from_config(config).unwrap();

        hybrid.add_strategy("test", Box::new(static_disc));
        assert_eq!(hybrid.strategy_count(), 1);
        assert!(hybrid.has_strategy("test"));

        hybrid.remove_strategy("test");
        assert_eq!(hybrid.strategy_count(), 0);
        assert!(!hybrid.has_strategy("test"));
    }
}
