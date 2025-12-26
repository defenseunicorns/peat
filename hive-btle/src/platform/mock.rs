//! Mock BLE adapter for testing
//!
//! This module provides a simulated BLE adapter that enables unit and integration
//! testing of HIVE mesh logic without requiring actual BLE hardware.
//!
//! ## Features
//!
//! - Simulated device discovery and advertising
//! - Configurable connection behavior (success, failure, latency)
//! - Event tracking for test assertions
//! - Multi-node simulation via shared state
//!
//! ## Example
//!
//! ```rust,no_run
//! use hive_btle::platform::mock::{MockBleAdapter, MockNetwork};
//! use hive_btle::platform::BleAdapter;
//! use hive_btle::config::{BleConfig, DiscoveryConfig};
//! use hive_btle::NodeId;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a shared network for multiple mock nodes
//! let network = MockNetwork::new();
//!
//! // Create two mock adapters on the same network
//! let mut adapter1 = MockBleAdapter::new(NodeId::new(0x111), network.clone());
//! let mut adapter2 = MockBleAdapter::new(NodeId::new(0x222), network.clone());
//!
//! // Initialize and start both adapters
//! adapter1.init(&BleConfig::default()).await?;
//! adapter2.init(&BleConfig::default()).await?;
//!
//! // Start advertising on adapter2 so it can be discovered
//! adapter2.start_advertising(&DiscoveryConfig::default()).await?;
//!
//! // Connect adapter1 to adapter2
//! let conn = adapter1.connect(&NodeId::new(0x222)).await?;
//! assert!(conn.is_alive());
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::config::{BleConfig, BlePhy, DiscoveryConfig};
use crate::error::{BleError, Result};
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DisconnectReason, DiscoveredDevice,
    DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::NodeId;

/// Shared network state for multiple mock adapters
///
/// Allows multiple `MockBleAdapter` instances to "see" each other and
/// simulate BLE discovery and connections.
#[derive(Clone, Default)]
pub struct MockNetwork {
    inner: Arc<MockNetworkInner>,
}

#[derive(Default)]
struct MockNetworkInner {
    /// All nodes currently advertising on the network
    advertising_nodes: RwLock<HashMap<NodeId, AdvertisingNode>>,
    /// Active connections (bidirectional)
    connections: RwLock<HashMap<(NodeId, NodeId), ConnectionState>>,
    /// Data sent between nodes
    data_queue: Mutex<HashMap<NodeId, Vec<DataPacket>>>,
}

/// An advertising node on the mock network
#[derive(Clone)]
struct AdvertisingNode {
    node_id: NodeId,
    address: String,
    name: Option<String>,
    rssi: i8,
    adv_data: Vec<u8>,
}

/// State of a connection
#[derive(Clone)]
struct ConnectionState {
    alive: Arc<AtomicBool>,
}

/// A data packet sent between nodes
#[derive(Clone)]
pub struct DataPacket {
    /// Source node
    pub from: NodeId,
    /// Destination node
    pub to: NodeId,
    /// Payload data
    pub data: Vec<u8>,
    /// When it was sent
    pub timestamp: Instant,
}

impl MockNetwork {
    /// Create a new mock network
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a node as advertising
    pub fn start_advertising(&self, node_id: NodeId, address: &str, name: Option<&str>) {
        let mut nodes = self.inner.advertising_nodes.write().unwrap();
        nodes.insert(
            node_id,
            AdvertisingNode {
                node_id,
                address: address.to_string(),
                name: name.map(|s| s.to_string()),
                rssi: -50, // Default good signal
                adv_data: vec![],
            },
        );
    }

    /// Stop advertising for a node
    pub fn stop_advertising(&self, node_id: &NodeId) {
        let mut nodes = self.inner.advertising_nodes.write().unwrap();
        nodes.remove(node_id);
    }

    /// Get all advertising nodes visible to a given node
    pub fn discover_nodes(&self, observer: &NodeId) -> Vec<DiscoveredDevice> {
        let nodes = self.inner.advertising_nodes.read().unwrap();
        nodes
            .values()
            .filter(|n| &n.node_id != observer)
            .map(|n| DiscoveredDevice {
                address: n.address.clone(),
                name: n.name.clone(),
                rssi: n.rssi,
                is_hive_node: true,
                node_id: Some(n.node_id),
                adv_data: n.adv_data.clone(),
            })
            .collect()
    }

    /// Establish a connection between two nodes
    pub fn connect(&self, from: &NodeId, to: &NodeId) -> Result<()> {
        // Check if target is advertising
        {
            let nodes = self.inner.advertising_nodes.read().unwrap();
            if !nodes.contains_key(to) {
                return Err(BleError::ConnectionFailed(format!(
                    "Node {} is not advertising",
                    to
                )));
            }
        }

        // Create connection state
        let state = ConnectionState {
            alive: Arc::new(AtomicBool::new(true)),
        };

        // Store bidirectionally
        let mut connections = self.inner.connections.write().unwrap();
        connections.insert((*from, *to), state.clone());
        connections.insert((*to, *from), state);

        Ok(())
    }

    /// Disconnect two nodes
    pub fn disconnect(&self, from: &NodeId, to: &NodeId) {
        let mut connections = self.inner.connections.write().unwrap();
        if let Some(state) = connections.remove(&(*from, *to)) {
            state.alive.store(false, Ordering::SeqCst);
        }
        if let Some(state) = connections.remove(&(*to, *from)) {
            state.alive.store(false, Ordering::SeqCst);
        }
    }

    /// Check if two nodes are connected
    pub fn is_connected(&self, from: &NodeId, to: &NodeId) -> bool {
        let connections = self.inner.connections.read().unwrap();
        connections
            .get(&(*from, *to))
            .is_some_and(|c| c.alive.load(Ordering::SeqCst))
    }

    /// Send data from one node to another
    pub fn send_data(&self, from: &NodeId, to: &NodeId, data: Vec<u8>) -> Result<()> {
        // Check if connection exists
        {
            let connections = self.inner.connections.read().unwrap();
            if !connections.contains_key(&(*from, *to)) {
                return Err(BleError::ConnectionFailed(format!(
                    "No connection from {} to {}",
                    from, to
                )));
            }
        }

        // Queue the data
        let mut queue = self.inner.data_queue.lock().unwrap();
        let packets = queue.entry(*to).or_default();
        packets.push(DataPacket {
            from: *from,
            to: *to,
            data,
            timestamp: Instant::now(),
        });

        Ok(())
    }

    /// Receive pending data for a node
    pub fn receive_data(&self, node_id: &NodeId) -> Vec<DataPacket> {
        let mut queue = self.inner.data_queue.lock().unwrap();
        queue.remove(node_id).unwrap_or_default()
    }

    /// Get all connected peers for a node
    pub fn connected_peers(&self, node_id: &NodeId) -> Vec<NodeId> {
        let connections = self.inner.connections.read().unwrap();
        connections
            .keys()
            .filter(|(from, _)| from == node_id)
            .map(|(_, to)| *to)
            .collect()
    }

    /// Clear all network state (for test cleanup)
    pub fn reset(&self) {
        self.inner.advertising_nodes.write().unwrap().clear();
        self.inner.connections.write().unwrap().clear();
        self.inner.data_queue.lock().unwrap().clear();
    }
}

/// Mock BLE connection
pub struct MockConnection {
    peer_id: NodeId,
    mtu: AtomicU16,
    phy: BlePhy,
    rssi: AtomicI8,
    alive: Arc<AtomicBool>,
    established_at: Instant,
}

impl Clone for MockConnection {
    fn clone(&self) -> Self {
        Self {
            peer_id: self.peer_id,
            mtu: AtomicU16::new(self.mtu.load(Ordering::SeqCst)),
            phy: self.phy,
            rssi: AtomicI8::new(self.rssi.load(Ordering::SeqCst)),
            alive: self.alive.clone(),
            established_at: self.established_at,
        }
    }
}

impl MockConnection {
    /// Create a new mock connection
    pub fn new(peer_id: NodeId, mtu: u16, phy: BlePhy) -> Self {
        Self {
            peer_id,
            mtu: AtomicU16::new(mtu),
            phy,
            rssi: AtomicI8::new(-50), // Default RSSI
            alive: Arc::new(AtomicBool::new(true)),
            established_at: Instant::now(),
        }
    }

    /// Kill this connection
    pub fn kill(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }

    /// Set RSSI value
    pub fn set_rssi(&self, rssi: i8) {
        self.rssi.store(rssi, Ordering::SeqCst);
    }

    /// Set MTU value
    pub fn set_mtu(&self, mtu: u16) {
        self.mtu.store(mtu, Ordering::SeqCst);
    }
}

impl BleConnection for MockConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn mtu(&self) -> u16 {
        self.mtu.load(Ordering::SeqCst)
    }

    fn phy(&self) -> BlePhy {
        self.phy
    }

    fn rssi(&self) -> Option<i8> {
        Some(self.rssi.load(Ordering::SeqCst))
    }

    fn connected_duration(&self) -> Duration {
        self.established_at.elapsed()
    }
}

/// Configuration for mock adapter behavior
#[derive(Clone, Debug)]
pub struct MockAdapterConfig {
    /// Simulate connection failures (0.0 = never, 1.0 = always)
    pub connection_failure_rate: f32,
    /// Simulated connection latency
    pub connection_latency: Duration,
    /// Simulated scan latency before discovering devices
    pub scan_latency: Duration,
    /// Support Coded PHY
    pub supports_coded_phy: bool,
    /// Support extended advertising
    pub supports_extended_advertising: bool,
    /// Maximum MTU
    pub max_mtu: u16,
    /// Maximum connections
    pub max_connections: u8,
}

impl Default for MockAdapterConfig {
    fn default() -> Self {
        Self {
            connection_failure_rate: 0.0,
            connection_latency: Duration::from_millis(50),
            scan_latency: Duration::from_millis(10),
            supports_coded_phy: true,
            supports_extended_advertising: true,
            max_mtu: 517,
            max_connections: 8,
        }
    }
}

/// Mock BLE adapter for testing
///
/// Provides a fully simulated BLE adapter that can be used in unit tests
/// without requiring actual BLE hardware.
pub struct MockBleAdapter {
    node_id: NodeId,
    network: MockNetwork,
    config: MockAdapterConfig,
    powered: AtomicBool,
    scanning: AtomicBool,
    advertising: AtomicBool,
    address: String,
    discovery_callback: Mutex<Option<DiscoveryCallback>>,
    connection_callback: Mutex<Option<ConnectionCallback>>,
    connections: RwLock<HashMap<NodeId, Arc<MockConnection>>>,
    /// Events recorded for test assertions
    events: Mutex<Vec<MockEvent>>,
}

/// Events recorded by the mock adapter for test assertions
#[derive(Clone, Debug)]
pub enum MockEvent {
    /// Adapter was initialized
    Initialized,
    /// Adapter was started
    Started,
    /// Adapter was stopped
    Stopped,
    /// Scanning for devices started
    ScanStarted,
    /// Scanning stopped
    ScanStopped,
    /// Advertising started
    AdvertisingStarted,
    /// Advertising stopped
    AdvertisingStopped,
    /// Connected to a peer
    Connected(NodeId),
    /// Disconnected from a peer
    Disconnected(NodeId, DisconnectReason),
    /// GATT service was registered
    GattServiceRegistered,
    /// GATT service was unregistered
    GattServiceUnregistered,
}

impl MockBleAdapter {
    /// Create a new mock adapter with default configuration
    pub fn new(node_id: NodeId, network: MockNetwork) -> Self {
        Self::with_config(node_id, network, MockAdapterConfig::default())
    }

    /// Create a new mock adapter with custom configuration
    pub fn with_config(node_id: NodeId, network: MockNetwork, config: MockAdapterConfig) -> Self {
        let address = format!(
            "00:11:22:{:02X}:{:02X}:{:02X}",
            (node_id.as_u32() >> 16) & 0xFF,
            (node_id.as_u32() >> 8) & 0xFF,
            node_id.as_u32() & 0xFF
        );
        Self {
            node_id,
            network,
            config,
            powered: AtomicBool::new(false),
            scanning: AtomicBool::new(false),
            advertising: AtomicBool::new(false),
            address,
            discovery_callback: Mutex::new(None),
            connection_callback: Mutex::new(None),
            connections: RwLock::new(HashMap::new()),
            events: Mutex::new(Vec::new()),
        }
    }

    /// Get recorded events for test assertions
    pub fn events(&self) -> Vec<MockEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Clear recorded events
    pub fn clear_events(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Record an event
    fn record_event(&self, event: MockEvent) {
        self.events.lock().unwrap().push(event);
    }

    /// Trigger discovery of nearby nodes
    ///
    /// Call this in tests to simulate device discovery.
    pub fn trigger_discovery(&self) {
        let devices = self.network.discover_nodes(&self.node_id);
        if let Some(ref callback) = *self.discovery_callback.lock().unwrap() {
            for device in devices {
                callback(device);
            }
        }
    }

    /// Simulate receiving data from a peer
    ///
    /// Call this in tests to inject data as if received over BLE.
    pub fn inject_data(&self, from: &NodeId, data: Vec<u8>) {
        if let Some(ref callback) = *self.connection_callback.lock().unwrap() {
            callback(*from, ConnectionEvent::DataReceived { data });
        }
    }

    /// Simulate a peer disconnecting
    pub fn simulate_disconnect(&self, peer_id: &NodeId, reason: DisconnectReason) {
        // Remove from our connections
        {
            let mut conns = self.connections.write().unwrap();
            if let Some(conn) = conns.remove(peer_id) {
                conn.kill();
            }
        }

        // Disconnect from network
        self.network.disconnect(&self.node_id, peer_id);

        // Notify callback
        if let Some(ref callback) = *self.connection_callback.lock().unwrap() {
            callback(*peer_id, ConnectionEvent::Disconnected { reason });
        }

        self.record_event(MockEvent::Disconnected(*peer_id, reason));
    }

    /// Get the node ID
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Check if scanning
    pub fn is_scanning(&self) -> bool {
        self.scanning.load(Ordering::SeqCst)
    }

    /// Check if advertising
    pub fn is_advertising(&self) -> bool {
        self.advertising.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl BleAdapter for MockBleAdapter {
    async fn init(&mut self, _config: &BleConfig) -> Result<()> {
        self.powered.store(true, Ordering::SeqCst);
        self.record_event(MockEvent::Initialized);
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        self.record_event(MockEvent::Started);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.scanning.store(false, Ordering::SeqCst);
        self.advertising.store(false, Ordering::SeqCst);
        self.network.stop_advertising(&self.node_id);
        self.record_event(MockEvent::Stopped);
        Ok(())
    }

    fn is_powered(&self) -> bool {
        self.powered.load(Ordering::SeqCst)
    }

    fn address(&self) -> Option<String> {
        Some(self.address.clone())
    }

    async fn start_scan(&self, _config: &DiscoveryConfig) -> Result<()> {
        self.scanning.store(true, Ordering::SeqCst);
        self.record_event(MockEvent::ScanStarted);

        // Optionally auto-discover after scan latency
        // In real tests, call trigger_discovery() manually for more control

        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        self.scanning.store(false, Ordering::SeqCst);
        self.record_event(MockEvent::ScanStopped);
        Ok(())
    }

    async fn start_advertising(&self, _config: &DiscoveryConfig) -> Result<()> {
        self.advertising.store(true, Ordering::SeqCst);
        self.network
            .start_advertising(self.node_id, &self.address, Some("HIVE"));
        self.record_event(MockEvent::AdvertisingStarted);
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<()> {
        self.advertising.store(false, Ordering::SeqCst);
        self.network.stop_advertising(&self.node_id);
        self.record_event(MockEvent::AdvertisingStopped);
        Ok(())
    }

    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>) {
        *self.discovery_callback.lock().unwrap() = callback;
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        // Check connection limit
        if self.connections.read().unwrap().len() >= self.config.max_connections as usize {
            return Err(BleError::ConnectionFailed(
                "Maximum connections reached".to_string(),
            ));
        }

        // Note: connection_latency is available in config for tests that want to
        // add delays, but we don't block by default in the mock

        // Establish connection via network
        self.network.connect(&self.node_id, peer_id)?;

        // Create connection object
        let conn = Arc::new(MockConnection::new(
            *peer_id,
            self.config.max_mtu,
            BlePhy::Le1M,
        ));

        // Store connection
        {
            let mut conns = self.connections.write().unwrap();
            conns.insert(*peer_id, conn.clone());
        }

        // Notify callback
        if let Some(ref callback) = *self.connection_callback.lock().unwrap() {
            callback(
                *peer_id,
                ConnectionEvent::Connected {
                    mtu: conn.mtu(),
                    phy: conn.phy(),
                },
            );
        }

        self.record_event(MockEvent::Connected(*peer_id));
        Ok(Box::new(conn.as_ref().clone()))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        self.simulate_disconnect(peer_id, DisconnectReason::LocalRequest);
        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        let conns = self.connections.read().unwrap();
        conns
            .get(peer_id)
            .filter(|c| c.is_alive())
            .map(|c| Box::new(c.as_ref().clone()) as Box<dyn BleConnection>)
    }

    fn peer_count(&self) -> usize {
        self.connections
            .read()
            .unwrap()
            .values()
            .filter(|c| c.is_alive())
            .count()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.connections
            .read()
            .unwrap()
            .iter()
            .filter(|(_, c)| c.is_alive())
            .map(|(id, _)| *id)
            .collect()
    }

    fn set_connection_callback(&mut self, callback: Option<ConnectionCallback>) {
        *self.connection_callback.lock().unwrap() = callback;
    }

    async fn register_gatt_service(&self) -> Result<()> {
        self.record_event(MockEvent::GattServiceRegistered);
        Ok(())
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        self.record_event(MockEvent::GattServiceUnregistered);
        Ok(())
    }

    fn supports_coded_phy(&self) -> bool {
        self.config.supports_coded_phy
    }

    fn supports_extended_advertising(&self) -> bool {
        self.config.supports_extended_advertising
    }

    fn max_mtu(&self) -> u16 {
        self.config.max_mtu
    }

    fn max_connections(&self) -> u8 {
        self.config.max_connections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_adapter_init() {
        let network = MockNetwork::new();
        let mut adapter = MockBleAdapter::new(NodeId::new(0x111), network);

        assert!(!adapter.is_powered());
        adapter.init(&BleConfig::default()).await.unwrap();
        assert!(adapter.is_powered());

        let events = adapter.events();
        assert!(matches!(events[0], MockEvent::Initialized));
    }

    #[tokio::test]
    async fn test_mock_network_discovery() {
        let network = MockNetwork::new();

        let mut adapter1 = MockBleAdapter::new(NodeId::new(0x111), network.clone());
        let mut adapter2 = MockBleAdapter::new(NodeId::new(0x222), network.clone());

        adapter1.init(&BleConfig::default()).await.unwrap();
        adapter2.init(&BleConfig::default()).await.unwrap();

        // Start advertising on adapter2
        adapter2
            .start_advertising(&DiscoveryConfig::default())
            .await
            .unwrap();

        // Discover from adapter1
        let devices = network.discover_nodes(&NodeId::new(0x111));
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].node_id, Some(NodeId::new(0x222)));
    }

    #[tokio::test]
    async fn test_mock_connection() {
        let network = MockNetwork::new();

        let mut adapter1 = MockBleAdapter::new(NodeId::new(0x111), network.clone());
        let mut adapter2 = MockBleAdapter::new(NodeId::new(0x222), network.clone());

        adapter1.init(&BleConfig::default()).await.unwrap();
        adapter2.init(&BleConfig::default()).await.unwrap();

        // Adapter2 must be advertising to accept connections
        adapter2
            .start_advertising(&DiscoveryConfig::default())
            .await
            .unwrap();

        // Connect adapter1 to adapter2
        let conn = adapter1.connect(&NodeId::new(0x222)).await.unwrap();
        assert!(conn.is_alive());
        assert_eq!(conn.peer_id(), &NodeId::new(0x222));

        // Verify connection tracking
        assert_eq!(adapter1.peer_count(), 1);
        assert!(adapter1.connected_peers().contains(&NodeId::new(0x222)));
    }

    #[tokio::test]
    async fn test_mock_disconnect() {
        let network = MockNetwork::new();

        let mut adapter1 = MockBleAdapter::new(NodeId::new(0x111), network.clone());
        let mut adapter2 = MockBleAdapter::new(NodeId::new(0x222), network.clone());

        adapter1.init(&BleConfig::default()).await.unwrap();
        adapter2.init(&BleConfig::default()).await.unwrap();
        adapter2
            .start_advertising(&DiscoveryConfig::default())
            .await
            .unwrap();

        let conn = adapter1.connect(&NodeId::new(0x222)).await.unwrap();
        assert!(conn.is_alive());

        // Disconnect
        adapter1.disconnect(&NodeId::new(0x222)).await.unwrap();
        assert_eq!(adapter1.peer_count(), 0);
    }

    #[tokio::test]
    async fn test_connection_limit() {
        let network = MockNetwork::new();

        let config = MockAdapterConfig {
            max_connections: 2,
            ..Default::default()
        };
        let mut adapter1 = MockBleAdapter::with_config(NodeId::new(0x111), network.clone(), config);
        adapter1.init(&BleConfig::default()).await.unwrap();

        // Create 3 other adapters
        for i in 2..=4 {
            let mut other = MockBleAdapter::new(NodeId::new(i * 0x111), network.clone());
            other.init(&BleConfig::default()).await.unwrap();
            other
                .start_advertising(&DiscoveryConfig::default())
                .await
                .unwrap();
        }

        // First two connections should succeed
        adapter1.connect(&NodeId::new(0x222)).await.unwrap();
        adapter1.connect(&NodeId::new(0x333)).await.unwrap();

        // Third should fail
        let result = adapter1.connect(&NodeId::new(0x444)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_event_tracking() {
        let network = MockNetwork::new();
        let mut adapter = MockBleAdapter::new(NodeId::new(0x111), network.clone());

        adapter.init(&BleConfig::default()).await.unwrap();
        adapter.start().await.unwrap();
        adapter
            .start_scan(&DiscoveryConfig::default())
            .await
            .unwrap();
        adapter.stop_scan().await.unwrap();
        adapter
            .start_advertising(&DiscoveryConfig::default())
            .await
            .unwrap();
        adapter.stop_advertising().await.unwrap();
        adapter.stop().await.unwrap();

        let events = adapter.events();
        assert!(matches!(events[0], MockEvent::Initialized));
        assert!(matches!(events[1], MockEvent::Started));
        assert!(matches!(events[2], MockEvent::ScanStarted));
        assert!(matches!(events[3], MockEvent::ScanStopped));
        assert!(matches!(events[4], MockEvent::AdvertisingStarted));
        assert!(matches!(events[5], MockEvent::AdvertisingStopped));
        assert!(matches!(events[6], MockEvent::Stopped));
    }

    #[tokio::test]
    async fn test_data_injection() {
        let network = MockNetwork::new();
        let mut adapter = MockBleAdapter::new(NodeId::new(0x111), network.clone());
        adapter.init(&BleConfig::default()).await.unwrap();

        // Track received data
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        adapter.set_connection_callback(Some(Box::new(move |node_id, event| {
            if let ConnectionEvent::DataReceived { data } = event {
                received_clone.lock().unwrap().push((node_id, data));
            }
        })));

        // Inject data
        adapter.inject_data(&NodeId::new(0x222), vec![1, 2, 3, 4]);

        let data = received.lock().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].0, NodeId::new(0x222));
        assert_eq!(data[0].1, vec![1, 2, 3, 4]);
    }
}
