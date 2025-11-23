//! Topology Manager for mesh connection lifecycle
//!
//! This module implements the TopologyManager which coordinates topology-driven
//! connection establishment by listening to topology events and managing transport
//! connections accordingly.

use super::{TopologyBuilder, TopologyConfig, TopologyEvent};
use crate::routing::DataPacket;
use hive_protocol::transport::{MeshConnection, MeshTransport, NodeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Retry state for a specific peer connection
#[derive(Debug, Clone)]
struct RetryState {
    /// Number of retry attempts made so far
    attempts: u32,
    /// When the next retry should be attempted
    next_retry: Instant,
}

/// Calculate exponential backoff delay for a given retry attempt
///
/// Uses the formula: min(initial_backoff * multiplier^attempts, max_backoff)
fn calculate_backoff(
    initial_backoff: Duration,
    max_backoff: Duration,
    backoff_multiplier: f64,
    attempts: u32,
) -> Duration {
    let multiplier = backoff_multiplier.powi(attempts as i32);
    let backoff_secs = initial_backoff.as_secs_f64() * multiplier;
    let capped_secs = backoff_secs.min(max_backoff.as_secs_f64());
    Duration::from_secs_f64(capped_secs)
}

/// Spawn a background task to retry selected peer connection with exponential backoff
fn spawn_peer_connection_retry(
    peer_id: String,
    transport: Arc<dyn MeshTransport>,
    peer_connection: Arc<RwLock<Option<Box<dyn MeshConnection>>>>,
    selected_peer_id: Arc<RwLock<Option<NodeId>>>,
    peer_retry_state: Arc<RwLock<Option<RetryState>>>,
    telemetry_buffer: Arc<RwLock<Vec<DataPacket>>>,
    config: TopologyConfig,
) {
    tokio::spawn(async move {
        let node_id = NodeId::new(peer_id.clone());

        loop {
            // Check current retry state
            let (attempts, sleep_duration) = {
                let retry_state = peer_retry_state.read().unwrap();
                match retry_state.as_ref() {
                    None => {
                        // No retry needed (might have been cleared by another task)
                        return;
                    }
                    Some(state) => {
                        if state.attempts >= config.max_retries {
                            warn!(
                                "Max retries ({}) reached for peer {}, giving up",
                                config.max_retries, peer_id
                            );
                            peer_retry_state.write().unwrap().take();
                            return;
                        }

                        // Calculate sleep duration until next retry
                        let now = Instant::now();
                        let sleep_duration = if now < state.next_retry {
                            state.next_retry.duration_since(now)
                        } else {
                            Duration::from_secs(0)
                        };

                        (state.attempts, sleep_duration)
                    }
                }
            };

            // Sleep until it's time to retry
            if sleep_duration > Duration::from_secs(0) {
                sleep(sleep_duration).await;
            }

            // Attempt connection
            info!(
                "Retrying connection to peer {} (attempt {}/{})",
                peer_id,
                attempts + 1,
                config.max_retries
            );

            match transport.connect(&node_id).await {
                Ok(conn) => {
                    *peer_connection.write().unwrap() = Some(conn);
                    *selected_peer_id.write().unwrap() = Some(node_id);
                    peer_retry_state.write().unwrap().take();
                    info!(
                        "Successfully connected to peer {} after {} retries",
                        peer_id, attempts
                    );

                    // Flush any buffered telemetry packets now that parent is available
                    TopologyManager::flush_buffer(&telemetry_buffer);

                    return;
                }
                Err(e) => {
                    warn!(
                        "Failed to connect to peer {} (attempt {}/{}): {}",
                        peer_id,
                        attempts + 1,
                        config.max_retries,
                        e
                    );

                    // Update retry state
                    let new_attempts = attempts + 1;
                    if new_attempts >= config.max_retries {
                        warn!(
                            "Max retries ({}) reached for peer {}, giving up",
                            config.max_retries, peer_id
                        );
                        peer_retry_state.write().unwrap().take();
                        return;
                    }

                    let backoff = calculate_backoff(
                        config.initial_backoff,
                        config.max_backoff,
                        config.backoff_multiplier,
                        new_attempts,
                    );

                    let next_retry = Instant::now() + backoff;
                    *peer_retry_state.write().unwrap() = Some(RetryState {
                        attempts: new_attempts,
                        next_retry,
                    });

                    debug!("Next retry for peer {} in {:?}", peer_id, backoff);
                }
            }
        }
    });
}

/// Spawn a background task to retry lateral peer connection with exponential backoff
fn spawn_lateral_connection_retry(
    peer_id: String,
    transport: Arc<dyn MeshTransport>,
    lateral_connections: Arc<RwLock<HashMap<String, Box<dyn MeshConnection>>>>,
    lateral_retry_state: Arc<RwLock<HashMap<String, RetryState>>>,
    config: TopologyConfig,
) {
    tokio::spawn(async move {
        let node_id = NodeId::new(peer_id.clone());

        loop {
            // Check current retry state
            let (attempts, sleep_duration) = {
                let retry_states = lateral_retry_state.read().unwrap();
                match retry_states.get(&peer_id) {
                    None => {
                        // No retry needed (might have been cleared)
                        return;
                    }
                    Some(state) => {
                        if state.attempts >= config.max_retries {
                            warn!(
                                "Max retries ({}) reached for lateral peer {}, giving up",
                                config.max_retries, peer_id
                            );
                            lateral_retry_state.write().unwrap().remove(&peer_id);
                            return;
                        }

                        // Calculate sleep duration until next retry
                        let now = Instant::now();
                        let sleep_duration = if now < state.next_retry {
                            state.next_retry.duration_since(now)
                        } else {
                            Duration::from_secs(0)
                        };

                        (state.attempts, sleep_duration)
                    }
                }
            };

            // Sleep until it's time to retry
            if sleep_duration > Duration::from_secs(0) {
                sleep(sleep_duration).await;
            }

            // Attempt connection
            info!(
                "Retrying connection to lateral peer {} (attempt {}/{})",
                peer_id,
                attempts + 1,
                config.max_retries
            );

            match transport.connect(&node_id).await {
                Ok(conn) => {
                    lateral_connections
                        .write()
                        .unwrap()
                        .insert(peer_id.clone(), conn);
                    lateral_retry_state.write().unwrap().remove(&peer_id);
                    info!(
                        "Successfully connected to lateral peer {} after {} retries",
                        peer_id, attempts
                    );
                    return;
                }
                Err(e) => {
                    warn!(
                        "Failed to connect to lateral peer {} (attempt {}/{}): {}",
                        peer_id,
                        attempts + 1,
                        config.max_retries,
                        e
                    );

                    // Update retry state
                    let new_attempts = attempts + 1;
                    if new_attempts >= config.max_retries {
                        warn!(
                            "Max retries ({}) reached for lateral peer {}, giving up",
                            config.max_retries, peer_id
                        );
                        lateral_retry_state.write().unwrap().remove(&peer_id);
                        return;
                    }

                    let backoff = calculate_backoff(
                        config.initial_backoff,
                        config.max_backoff,
                        config.backoff_multiplier,
                        new_attempts,
                    );

                    let next_retry = Instant::now() + backoff;
                    lateral_retry_state.write().unwrap().insert(
                        peer_id.clone(),
                        RetryState {
                            attempts: new_attempts,
                            next_retry,
                        },
                    );

                    debug!("Next retry for lateral peer {} in {:?}", peer_id, backoff);
                }
            }
        }
    });
}

/// Topology Manager
///
/// Manages mesh connections based on topology formation events.
/// Wraps a TopologyBuilder and MeshTransport to automatically establish
/// and tear down connections as the topology changes.
///
/// # Architecture
///
/// - Subscribes to topology events from TopologyBuilder
/// - Reacts to PeerSelected/Changed/Lost events
/// - Establishes peer connections via MeshTransport
/// - Tears down stale connections
/// - Implements exponential backoff for connection retries
///
/// # Example
///
/// ```ignore
/// use hive_mesh::topology::{TopologyManager, TopologyBuilder};
/// use hive_protocol::transport::MeshTransport;
///
/// let builder = TopologyBuilder::new(...);
/// let transport: Arc<dyn MeshTransport> = ...;
/// let manager = TopologyManager::new(builder, transport);
///
/// manager.start().await?;
/// ```
pub struct TopologyManager {
    /// Topology builder for peer selection
    builder: TopologyBuilder,

    /// Transport abstraction for connections
    transport: Arc<dyn MeshTransport>,

    /// Current peer connection (if any)
    peer_connection: Arc<RwLock<Option<Box<dyn MeshConnection>>>>,

    /// Current selected peer node ID (if any)
    selected_peer_id: Arc<RwLock<Option<NodeId>>>,

    /// Lateral peer connections (same hierarchy level)
    lateral_connections: Arc<RwLock<HashMap<String, Box<dyn MeshConnection>>>>,

    /// Retry state for selected peer
    peer_retry_state: Arc<RwLock<Option<RetryState>>>,

    /// Retry state for lateral peers
    lateral_retry_state: Arc<RwLock<HashMap<String, RetryState>>>,

    /// Telemetry buffer for packets during parent transitions
    telemetry_buffer: Arc<RwLock<Vec<DataPacket>>>,

    /// Background task handle
    task_handle: RwLock<Option<JoinHandle<()>>>,
}

impl TopologyManager {
    /// Create a new topology manager
    ///
    /// # Arguments
    ///
    /// * `builder` - TopologyBuilder for peer selection
    /// * `transport` - Transport abstraction for connections
    pub fn new(builder: TopologyBuilder, transport: Arc<dyn MeshTransport>) -> Self {
        Self {
            builder,
            transport,
            peer_connection: Arc::new(RwLock::new(None)),
            selected_peer_id: Arc::new(RwLock::new(None)),
            lateral_connections: Arc::new(RwLock::new(HashMap::new())),
            peer_retry_state: Arc::new(RwLock::new(None)),
            lateral_retry_state: Arc::new(RwLock::new(HashMap::new())),
            telemetry_buffer: Arc::new(RwLock::new(Vec::new())),
            task_handle: RwLock::new(None),
        }
    }

    /// Start topology management
    ///
    /// Starts both the topology builder and the event listener that manages connections.
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Start the transport
        self.transport.start().await?;

        // Start the topology builder
        self.builder.start().await;

        // Subscribe to topology events
        if let Some(rx) = self.builder.subscribe() {
            let transport = self.transport.clone();
            let peer_connection = self.peer_connection.clone();
            let selected_peer_id = self.selected_peer_id.clone();
            let lateral_connections = self.lateral_connections.clone();
            let peer_retry_state = self.peer_retry_state.clone();
            let lateral_retry_state = self.lateral_retry_state.clone();
            let telemetry_buffer = self.telemetry_buffer.clone();
            let builder = self.builder.clone();
            let config = self.builder.config().clone();

            let handle = tokio::spawn(async move {
                Self::event_loop(
                    rx,
                    transport,
                    peer_connection,
                    selected_peer_id,
                    lateral_connections,
                    peer_retry_state,
                    lateral_retry_state,
                    telemetry_buffer,
                    builder,
                    config,
                )
                .await;
            });

            *self.task_handle.write().unwrap() = Some(handle);
        }

        Ok(())
    }

    /// Stop topology management
    ///
    /// Stops the topology builder and disconnects from all peers.
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Abort the event loop task
        if let Some(handle) = self.task_handle.write().unwrap().take() {
            handle.abort();
        }

        // Stop the topology builder
        self.builder.stop().await;

        // Disconnect from current selected peer
        let current_selected_peer_id = self.selected_peer_id.write().unwrap().take();
        if let Some(selected_peer_id) = current_selected_peer_id {
            if let Err(e) = self.transport.disconnect(&selected_peer_id).await {
                warn!("Failed to disconnect from selected peer during stop: {}", e);
            }
        }

        // Disconnect from all lateral peers
        let lateral_peer_ids: Vec<String> = self
            .lateral_connections
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect();

        for peer_id in lateral_peer_ids {
            let node_id = NodeId::new(peer_id.clone());
            if let Err(e) = self.transport.disconnect(&node_id).await {
                warn!(
                    "Failed to disconnect from lateral peer {} during stop: {}",
                    peer_id, e
                );
            }
        }

        self.lateral_connections.write().unwrap().clear();

        // Stop the transport
        self.transport.stop().await?;

        Ok(())
    }

    /// Get current selected peer node ID
    pub fn get_selected_peer_id(&self) -> Option<NodeId> {
        self.selected_peer_id.read().unwrap().clone()
    }

    /// Check if currently connected to a specific peer
    pub fn is_connected_to_peer(&self, node_id: &NodeId) -> bool {
        self.selected_peer_id
            .read()
            .unwrap()
            .as_ref()
            .map(|id| id == node_id)
            .unwrap_or(false)
    }

    /// Get the underlying topology builder
    pub fn builder(&self) -> &TopologyBuilder {
        &self.builder
    }

    /// Get all current lateral peer node IDs
    pub fn get_lateral_peer_ids(&self) -> Vec<String> {
        self.lateral_connections
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// Get the number of lateral peer connections
    pub fn lateral_peer_count(&self) -> usize {
        self.lateral_connections.read().unwrap().len()
    }

    /// Send a telemetry packet
    ///
    /// If parent connection is available, sends immediately.
    /// Otherwise, buffers the packet for later delivery (up to max_telemetry_buffer_size).
    ///
    /// # Arguments
    ///
    /// * `packet` - The telemetry packet to send
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if packet was sent immediately
    /// - `Ok(false)` if packet was buffered
    /// - `Err` if buffer is full and buffering is disabled
    pub fn send_telemetry(&self, packet: DataPacket) -> Result<bool, String> {
        let has_parent = self.selected_peer_id.read().unwrap().is_some();

        if has_parent {
            // Parent connection available - attempt to send immediately
            // For now, just return true (actual sending would go through MeshConnection)
            // TODO: Implement actual packet sending through MeshConnection
            info!(
                "Sending telemetry packet {} immediately to parent",
                packet.packet_id
            );
            Ok(true)
        } else {
            // No parent connection - buffer the packet
            let max_buffer_size = self.builder.config().max_telemetry_buffer_size;

            if max_buffer_size == 0 {
                return Err(
                    "Telemetry buffering is disabled (max_telemetry_buffer_size = 0)".to_string(),
                );
            }

            let mut buffer = self.telemetry_buffer.write().unwrap();

            if buffer.len() >= max_buffer_size {
                // Buffer is full - drop oldest packet (FIFO)
                buffer.remove(0);
                warn!(
                    "Telemetry buffer full ({}), dropping oldest packet",
                    max_buffer_size
                );
            }

            info!(
                "Buffering telemetry packet {} (buffer size: {}/{})",
                packet.packet_id,
                buffer.len() + 1,
                max_buffer_size
            );
            buffer.push(packet);
            Ok(false)
        }
    }

    /// Get current telemetry buffer size
    pub fn telemetry_buffer_size(&self) -> usize {
        self.telemetry_buffer.read().unwrap().len()
    }

    /// Flush telemetry buffer (helper for event_loop)
    fn flush_buffer(telemetry_buffer: &Arc<RwLock<Vec<DataPacket>>>) {
        let buffer_size = {
            let buffer = telemetry_buffer.read().unwrap();
            buffer.len()
        };

        if buffer_size == 0 {
            return;
        }

        info!(
            "Flushing {} buffered telemetry packets to new parent",
            buffer_size
        );

        // Take all buffered packets
        let buffered_packets: Vec<DataPacket> = {
            let mut buffer = telemetry_buffer.write().unwrap();
            buffer.drain(..).collect()
        };

        // TODO: Implement actual packet sending through MeshConnection
        // For now, just log that we would send them
        for packet in buffered_packets {
            debug!("Flushing telemetry packet {} to parent", packet.packet_id);
        }

        info!("Successfully flushed {} telemetry packets", buffer_size);
    }

    /// Event processing loop
    ///
    /// Listens to topology events and manages connections accordingly.
    #[allow(clippy::too_many_arguments)]
    async fn event_loop(
        mut rx: mpsc::UnboundedReceiver<TopologyEvent>,
        transport: Arc<dyn MeshTransport>,
        peer_connection: Arc<RwLock<Option<Box<dyn MeshConnection>>>>,
        selected_peer_id: Arc<RwLock<Option<NodeId>>>,
        lateral_connections: Arc<RwLock<HashMap<String, Box<dyn MeshConnection>>>>,
        peer_retry_state: Arc<RwLock<Option<RetryState>>>,
        lateral_retry_state: Arc<RwLock<HashMap<String, RetryState>>>,
        telemetry_buffer: Arc<RwLock<Vec<DataPacket>>>,
        builder: TopologyBuilder,
        config: TopologyConfig,
    ) {
        while let Some(event) = rx.recv().await {
            match event {
                TopologyEvent::PeerSelected {
                    selected_peer_id: new_peer_id,
                    ..
                } => {
                    info!("Peer selected: {}", new_peer_id);
                    let node_id = NodeId::new(new_peer_id.clone());

                    // Connect to the selected peer
                    match transport.connect(&node_id).await {
                        Ok(conn) => {
                            *peer_connection.write().unwrap() = Some(conn);
                            *selected_peer_id.write().unwrap() = Some(node_id);
                            peer_retry_state.write().unwrap().take(); // Clear any retry state
                            info!("Successfully connected to peer: {}", new_peer_id);

                            // Flush any buffered telemetry packets now that parent is available
                            Self::flush_buffer(&telemetry_buffer);
                        }
                        Err(e) => {
                            warn!("Failed to connect to peer {}: {}", new_peer_id, e);

                            // Initialize retry state and spawn retry task
                            if config.max_retries > 0 {
                                let backoff = calculate_backoff(
                                    config.initial_backoff,
                                    config.max_backoff,
                                    config.backoff_multiplier,
                                    0, // First attempt
                                );

                                *peer_retry_state.write().unwrap() = Some(RetryState {
                                    attempts: 0,
                                    next_retry: Instant::now() + backoff,
                                });

                                debug!("Scheduled retry for peer {} in {:?}", new_peer_id, backoff);

                                spawn_peer_connection_retry(
                                    new_peer_id,
                                    transport.clone(),
                                    peer_connection.clone(),
                                    selected_peer_id.clone(),
                                    peer_retry_state.clone(),
                                    telemetry_buffer.clone(),
                                    config.clone(),
                                );
                            }
                        }
                    }
                }

                TopologyEvent::PeerChanged {
                    old_peer_id,
                    new_peer_id,
                    ..
                } => {
                    info!("Selected peer changed: {} -> {}", old_peer_id, new_peer_id);

                    // Clear any existing retry state for old peer
                    peer_retry_state.write().unwrap().take();

                    // Disconnect from old peer
                    let old_id = NodeId::new(old_peer_id.clone());
                    if let Err(e) = transport.disconnect(&old_id).await {
                        warn!("Failed to disconnect from old peer {}: {}", old_peer_id, e);
                    }

                    // Connect to new peer
                    let new_id = NodeId::new(new_peer_id.clone());
                    match transport.connect(&new_id).await {
                        Ok(conn) => {
                            *peer_connection.write().unwrap() = Some(conn);
                            *selected_peer_id.write().unwrap() = Some(new_id);
                            info!("Successfully changed to peer: {}", new_peer_id);

                            // Flush any buffered telemetry packets now that new parent is available
                            Self::flush_buffer(&telemetry_buffer);
                        }
                        Err(e) => {
                            warn!("Failed to connect to new peer {}: {}", new_peer_id, e);

                            // Initialize retry state and spawn retry task
                            if config.max_retries > 0 {
                                let backoff = calculate_backoff(
                                    config.initial_backoff,
                                    config.max_backoff,
                                    config.backoff_multiplier,
                                    0, // First attempt
                                );

                                *peer_retry_state.write().unwrap() = Some(RetryState {
                                    attempts: 0,
                                    next_retry: Instant::now() + backoff,
                                });

                                debug!(
                                    "Scheduled retry for new peer {} in {:?}",
                                    new_peer_id, backoff
                                );

                                spawn_peer_connection_retry(
                                    new_peer_id,
                                    transport.clone(),
                                    peer_connection.clone(),
                                    selected_peer_id.clone(),
                                    peer_retry_state.clone(),
                                    telemetry_buffer.clone(),
                                    config.clone(),
                                );
                            }
                        }
                    }
                }

                TopologyEvent::PeerLost { lost_peer_id } => {
                    info!("Selected peer lost: {}", lost_peer_id);

                    // Clear peer connection
                    *peer_connection.write().unwrap() = None;
                    *selected_peer_id.write().unwrap() = None;

                    // Disconnect from lost peer
                    let node_id = NodeId::new(lost_peer_id.clone());
                    if let Err(e) = transport.disconnect(&node_id).await {
                        warn!(
                            "Failed to disconnect from lost peer {}: {}",
                            lost_peer_id, e
                        );
                    }

                    // Notify about telemetry buffering during parent transition
                    info!("Telemetry will be buffered until new parent connection is established");

                    // Trigger immediate parent re-selection
                    info!("Triggering immediate parent re-selection after peer loss");
                    builder.reevaluate_peer().await;

                    debug!("Cleared connection to lost peer: {}", lost_peer_id);
                }

                TopologyEvent::PeerAdded { linked_peer_id } => {
                    info!("Linked peer added: {}", linked_peer_id);
                    // Linked peers connect TO us, so no action needed here
                    // The transport layer handles incoming connections automatically
                }

                TopologyEvent::PeerRemoved { linked_peer_id } => {
                    info!("Linked peer removed (beacon expired): {}", linked_peer_id);

                    // Disconnect from stale linked peer
                    let node_id = NodeId::new(linked_peer_id.clone());
                    if transport.is_connected(&node_id) {
                        if let Err(e) = transport.disconnect(&node_id).await {
                            warn!(
                                "Failed to disconnect from stale linked peer {}: {}",
                                linked_peer_id, e
                            );
                        } else {
                            debug!("Disconnected from stale linked peer: {}", linked_peer_id);
                        }
                    }
                }

                TopologyEvent::LateralPeerDiscovered { peer_id, .. } => {
                    info!("Lateral peer discovered: {}", peer_id);

                    // Check if we've reached the maximum lateral connections limit
                    let max_lateral = builder.config().max_lateral_connections;
                    let current_count = lateral_connections.read().unwrap().len();

                    if let Some(max) = max_lateral {
                        if current_count >= max {
                            debug!(
                                "Skipping lateral peer {} - at connection limit ({}/{})",
                                peer_id, current_count, max
                            );
                            return;
                        }
                    }

                    // Connect to lateral peer for O(n²) mesh within same hierarchy level
                    let node_id = NodeId::new(peer_id.clone());
                    match transport.connect(&node_id).await {
                        Ok(conn) => {
                            lateral_connections
                                .write()
                                .unwrap()
                                .insert(peer_id.clone(), conn);
                            lateral_retry_state.write().unwrap().remove(&peer_id); // Clear any retry state
                            info!(
                                "Connected to lateral peer: {} ({}/{})",
                                peer_id,
                                current_count + 1,
                                max_lateral
                                    .map(|m| m.to_string())
                                    .unwrap_or_else(|| "unlimited".to_string())
                            );
                        }
                        Err(e) => {
                            warn!("Failed to connect to lateral peer {}: {}", peer_id, e);

                            // Initialize retry state and spawn retry task
                            if config.max_retries > 0 {
                                let backoff = calculate_backoff(
                                    config.initial_backoff,
                                    config.max_backoff,
                                    config.backoff_multiplier,
                                    0, // First attempt
                                );

                                lateral_retry_state.write().unwrap().insert(
                                    peer_id.clone(),
                                    RetryState {
                                        attempts: 0,
                                        next_retry: Instant::now() + backoff,
                                    },
                                );

                                debug!(
                                    "Scheduled retry for lateral peer {} in {:?}",
                                    peer_id, backoff
                                );

                                spawn_lateral_connection_retry(
                                    peer_id,
                                    transport.clone(),
                                    lateral_connections.clone(),
                                    lateral_retry_state.clone(),
                                    config.clone(),
                                );
                            }
                        }
                    }
                }

                TopologyEvent::LateralPeerLost { peer_id } => {
                    info!("Lateral peer lost: {}", peer_id);

                    // Clear any retry state for this peer
                    lateral_retry_state.write().unwrap().remove(&peer_id);

                    // Disconnect from lost lateral peer
                    if lateral_connections.read().unwrap().contains_key(&peer_id) {
                        lateral_connections.write().unwrap().remove(&peer_id);

                        let node_id = NodeId::new(peer_id.clone());
                        if let Err(e) = transport.disconnect(&node_id).await {
                            warn!("Failed to disconnect from lateral peer {}: {}", peer_id, e);
                        } else {
                            debug!("Disconnected from lateral peer: {}", peer_id);
                        }
                    }
                }

                TopologyEvent::RoleChanged { old_role, new_role } => {
                    info!("Role changed: {:?} -> {:?}", old_role, new_role);
                    // Role changes may affect connection patterns (future work)
                }

                TopologyEvent::LevelChanged {
                    old_level,
                    new_level,
                } => {
                    info!(
                        "Hierarchy level changed: {:?} -> {:?}",
                        old_level, new_level
                    );
                    // Level changes may require connection reorganization (future work)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hive_protocol::transport::{
        MeshConnection as MeshConnectionTrait, MeshTransport, NodeId, Result,
    };
    use std::sync::Arc;

    // Mock transport for testing
    struct MockTransport {
        started: Arc<RwLock<bool>>,
        stopped: Arc<RwLock<bool>>,
        connections: Arc<RwLock<Vec<NodeId>>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                started: Arc::new(RwLock::new(false)),
                stopped: Arc::new(RwLock::new(false)),
                connections: Arc::new(RwLock::new(Vec::new())),
            }
        }

        fn is_started(&self) -> bool {
            *self.started.read().unwrap()
        }

        fn is_stopped(&self) -> bool {
            *self.stopped.read().unwrap()
        }

        fn has_connection(&self, node_id: &NodeId) -> bool {
            self.connections
                .read()
                .unwrap()
                .iter()
                .any(|id| id == node_id)
        }
    }

    struct MockConnection {
        peer_id: NodeId,
    }

    impl MeshConnectionTrait for MockConnection {
        fn peer_id(&self) -> &NodeId {
            &self.peer_id
        }

        fn is_alive(&self) -> bool {
            true
        }
    }

    #[async_trait::async_trait]
    impl MeshTransport for MockTransport {
        async fn start(&self) -> Result<()> {
            *self.started.write().unwrap() = true;
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            *self.stopped.write().unwrap() = true;
            Ok(())
        }

        async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn MeshConnectionTrait>> {
            self.connections.write().unwrap().push(peer_id.clone());
            Ok(Box::new(MockConnection {
                peer_id: peer_id.clone(),
            }))
        }

        async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
            self.connections.write().unwrap().retain(|id| id != peer_id);
            Ok(())
        }

        fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn MeshConnectionTrait>> {
            if self.has_connection(peer_id) {
                Some(Box::new(MockConnection {
                    peer_id: peer_id.clone(),
                }))
            } else {
                None
            }
        }

        fn peer_count(&self) -> usize {
            self.connections.read().unwrap().len()
        }

        fn connected_peers(&self) -> Vec<NodeId> {
            self.connections.read().unwrap().clone()
        }
    }

    // Minimal test that doesn't require BeaconObserver
    #[test]
    fn test_node_id_api() {
        let node_id1 = NodeId::new("test-node".to_string());
        let node_id2 = NodeId::new("test-node".to_string());
        let node_id3 = NodeId::new("other-node".to_string());

        assert_eq!(node_id1, node_id2);
        assert_ne!(node_id1, node_id3);
    }

    #[test]
    fn test_mock_transport_creation() {
        let transport = MockTransport::new();
        assert!(!transport.is_started());
        assert!(!transport.is_stopped());
        assert_eq!(transport.peer_count(), 0);
    }

    // Tests for exponential backoff calculation
    #[test]
    fn test_calculate_backoff_first_attempt() {
        let initial = Duration::from_secs(1);
        let max = Duration::from_secs(60);
        let multiplier = 2.0;

        let backoff = calculate_backoff(initial, max, multiplier, 0);
        assert_eq!(backoff, initial);
    }

    #[test]
    fn test_calculate_backoff_exponential_growth() {
        let initial = Duration::from_secs(1);
        let max = Duration::from_secs(60);
        let multiplier = 2.0;

        let backoff1 = calculate_backoff(initial, max, multiplier, 1);
        assert_eq!(backoff1, Duration::from_secs(2));

        let backoff2 = calculate_backoff(initial, max, multiplier, 2);
        assert_eq!(backoff2, Duration::from_secs(4));

        let backoff3 = calculate_backoff(initial, max, multiplier, 3);
        assert_eq!(backoff3, Duration::from_secs(8));
    }

    #[test]
    fn test_calculate_backoff_max_cap() {
        let initial = Duration::from_secs(1);
        let max = Duration::from_secs(10);
        let multiplier = 2.0;

        // After several attempts, should cap at max
        let backoff = calculate_backoff(initial, max, multiplier, 10);
        assert_eq!(backoff, max);
    }

    #[test]
    fn test_calculate_backoff_custom_multiplier() {
        let initial = Duration::from_secs(1);
        let max = Duration::from_secs(100);
        let multiplier = 3.0;

        let backoff1 = calculate_backoff(initial, max, multiplier, 1);
        assert_eq!(backoff1, Duration::from_secs(3));

        let backoff2 = calculate_backoff(initial, max, multiplier, 2);
        assert_eq!(backoff2, Duration::from_secs(9));
    }

    // Test for TopologyConfig defaults
    #[test]
    fn test_topology_config_defaults() {
        let config = TopologyConfig::default();
        assert_eq!(config.max_telemetry_buffer_size, 100);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.max_backoff, Duration::from_secs(60));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.max_lateral_connections, Some(10));
    }
}
