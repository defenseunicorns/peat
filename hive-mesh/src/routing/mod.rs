//! Selective data routing for hierarchical mesh networks
//!
//! This module implements intelligent data routing that prevents flooding
//! while ensuring proper hierarchical aggregation. The SelectiveRouter
//! determines:
//! - Whether a node should consume (process) incoming data
//! - Whether data should be forwarded up/down the hierarchy
//! - Which peer should receive forwarded data
//!
//! # Architecture
//!
//! The router integrates with TopologyState to make hierarchy-aware routing
//! decisions based on:
//! - Node's hierarchy level (Squad, Platoon, Company, etc.)
//! - Node's role (Leader, Member, Standalone)
//! - Data flow direction (Upward telemetry vs Downward commands)
//! - Data type (Telemetry, Command, Status)
//!
//! # Data Flow Patterns
//!
//! **Upward (Telemetry Aggregation)**
//! - Leaf nodes: Consume locally + Forward to selected peer (parent)
//! - Intermediate nodes: Aggregate from linked peers + Forward up
//! - HQ nodes: Consume aggregated data (no forwarding)
//!
//! **Downward (Command Dissemination)**
//! - HQ nodes: Forward to all linked peers (children)
//! - Intermediate nodes: Consume if targeted + Forward to children
//! - Leaf nodes: Consume if targeted (no forwarding)
//!
//! **Lateral (Coordination)**
//! - Leaders: May exchange data with lateral peers at same level
//! - Members: Typically no lateral communication
//!
//! # Example
//!
//! ```ignore
//! use hive_mesh::routing::{SelectiveRouter, DataPacket, DataDirection};
//! use hive_mesh::topology::TopologyState;
//!
//! let router = SelectiveRouter::new();
//! let state = get_topology_state();
//!
//! // Decide if we should process incoming telemetry
//! let packet = DataPacket::telemetry("node-123", vec![1, 2, 3]);
//! if router.should_consume(&packet, &state) {
//!     process_telemetry(&packet);
//! }
//!
//! // Decide if we should forward data upward
//! if router.should_forward(&packet, &state) {
//!     if let Some(next) = router.next_hop(&packet, &state) {
//!         forward_to_peer(&next, &packet);
//!     }
//! }
//! ```

mod packet;
mod router;

pub use packet::{DataDirection, DataPacket, DataType};
pub use router::{RoutingDecision, SelectiveRouter};
