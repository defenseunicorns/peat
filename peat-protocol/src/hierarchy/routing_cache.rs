//! Routing cache for hierarchical message routing
//!
//! This module provides a high-performance caching layer for routing lookups
//! to avoid repeated Ditto queries. Uses RwLock for concurrent read access.

use crate::storage::ditto_store::DittoStore;
use crate::Result;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument};

/// Routing information cache for hierarchical operations
///
/// Caches node→cell and cell→zone mappings to avoid repeated Ditto queries.
/// Uses RwLock for concurrent read access with occasional write updates.
pub struct RoutingCache {
    /// Maps node_id to cell_id
    node_to_cell: Arc<RwLock<HashMap<String, String>>>,
    /// Maps cell_id to zone_id
    cell_to_zone: Arc<RwLock<HashMap<String, String>>>,
    /// Last time cache was refreshed
    last_refresh: Arc<RwLock<Instant>>,
    /// How often to refresh the cache
    refresh_interval: Duration,
}

impl RoutingCache {
    /// Create a new routing cache with specified refresh interval
    ///
    /// # Arguments
    /// * `refresh_interval` - How often to refresh the cache from storage
    ///
    /// # Example
    /// ```
    /// use std::time::Duration;
    /// use peat_protocol::hierarchy::RoutingCache;
    ///
    /// let cache = RoutingCache::new(Duration::from_secs(30));
    /// ```
    pub fn new(refresh_interval: Duration) -> Self {
        Self {
            node_to_cell: Arc::new(RwLock::new(HashMap::new())),
            cell_to_zone: Arc::new(RwLock::new(HashMap::new())),
            last_refresh: Arc::new(RwLock::new(Instant::now())),
            refresh_interval,
        }
    }

    /// Get the cell ID for a given node
    ///
    /// Returns None if the node is not assigned to any cell.
    #[instrument(skip(self))]
    pub fn get_node_cell(&self, node_id: &str) -> Option<String> {
        let cache = self.node_to_cell.read().unwrap();
        cache.get(node_id).cloned()
    }

    /// Get the zone ID for a given cell
    ///
    /// Returns None if the cell is not assigned to any zone.
    #[instrument(skip(self))]
    pub fn get_cell_zone(&self, cell_id: &str) -> Option<String> {
        let cache = self.cell_to_zone.read().unwrap();
        cache.get(cell_id).cloned()
    }

    /// Check if cache needs refresh based on refresh_interval
    pub fn needs_refresh(&self) -> bool {
        let last_refresh = self.last_refresh.read().unwrap();
        last_refresh.elapsed() >= self.refresh_interval
    }

    /// Refresh cache from storage
    ///
    /// Queries Ditto for all node and cell assignments and updates the cache.
    #[instrument(skip(self, store))]
    pub async fn refresh(&self, store: &DittoStore) -> Result<()> {
        info!("Refreshing routing cache");

        // Query all nodes for their cell assignments
        let nodes_query = "cell_id != null";
        let node_docs = store.query("node_states", nodes_query).await?;

        let mut node_to_cell_map = HashMap::new();
        for doc in node_docs {
            if let (Some(node_id), Some(cell_id)) = (
                doc.get("id").and_then(|v| v.as_str()),
                doc.get("cell_id").and_then(|v| v.as_str()),
            ) {
                node_to_cell_map.insert(node_id.to_string(), cell_id.to_string());
            }
        }

        // Query all cells for their zone assignments
        let cells_query = "zone_id != null";
        let cell_docs = store.query("cells", cells_query).await?;

        let mut cell_to_zone_map = HashMap::new();
        for doc in cell_docs {
            if let (Some(cell_id), Some(zone_id)) = (
                doc.get("cell_id").and_then(|v| v.as_str()),
                doc.get("zone_id").and_then(|v| v.as_str()),
            ) {
                cell_to_zone_map.insert(cell_id.to_string(), zone_id.to_string());
            }
        }

        // Update caches with write locks
        {
            let mut node_cache = self.node_to_cell.write().unwrap();
            *node_cache = node_to_cell_map;
        }
        {
            let mut cell_cache = self.cell_to_zone.write().unwrap();
            *cell_cache = cell_to_zone_map;
        }
        {
            let mut last_refresh = self.last_refresh.write().unwrap();
            *last_refresh = Instant::now();
        }

        debug!(
            "Cache refreshed: {} nodes, {} cells",
            self.node_to_cell.read().unwrap().len(),
            self.cell_to_zone.read().unwrap().len()
        );

        Ok(())
    }

    /// Manually invalidate the cache, forcing next access to refresh
    pub fn invalidate(&self) {
        let mut last_refresh = self.last_refresh.write().unwrap();
        *last_refresh = Instant::now() - self.refresh_interval - Duration::from_secs(1);
        info!("Routing cache invalidated");
    }

    /// Get cache statistics for monitoring
    pub fn stats(&self) -> CacheStats {
        let node_cache = self.node_to_cell.read().unwrap();
        let cell_cache = self.cell_to_zone.read().unwrap();
        let last_refresh = self.last_refresh.read().unwrap();

        CacheStats {
            node_to_cell_entries: node_cache.len(),
            cell_to_zone_entries: cell_cache.len(),
            age: last_refresh.elapsed(),
            needs_refresh: self.needs_refresh(),
        }
    }
}

/// Statistics about the routing cache
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of node→cell mappings in cache
    pub node_to_cell_entries: usize,
    /// Number of cell→zone mappings in cache
    pub cell_to_zone_entries: usize,
    /// Age of cache since last refresh
    pub age: Duration,
    /// Whether cache needs refresh
    pub needs_refresh: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_cache_creation() {
        let cache = RoutingCache::new(Duration::from_secs(30));
        // Newly created cache doesn't need refresh yet
        assert!(!cache.needs_refresh());

        let stats = cache.stats();
        assert_eq!(stats.node_to_cell_entries, 0);
        assert_eq!(stats.cell_to_zone_entries, 0);
    }

    #[test]
    fn test_cache_empty_lookups() {
        let cache = RoutingCache::new(Duration::from_secs(30));
        assert_eq!(cache.get_node_cell("node1"), None);
        assert_eq!(cache.get_cell_zone("cell1"), None);
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = RoutingCache::new(Duration::from_secs(30));

        // Initially should not need refresh (just created)
        std::thread::sleep(Duration::from_millis(100));

        // Invalidate and check
        cache.invalidate();
        assert!(cache.needs_refresh());
    }

    #[test]
    fn test_cache_stats() {
        let cache = RoutingCache::new(Duration::from_secs(30));
        let stats = cache.stats();

        assert_eq!(stats.node_to_cell_entries, 0);
        assert_eq!(stats.cell_to_zone_entries, 0);
        assert!(stats.age < Duration::from_secs(1));
    }
}
