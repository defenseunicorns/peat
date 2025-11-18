//! Hierarchical Aggregation Coordinator
//!
//! This module provides the coordinator layer for hierarchical state aggregation,
//! implementing ADR-021 document-oriented architecture principles.
//!
//! # Architecture
//!
//! The HierarchicalAggregator sits between the application (hive-sim) and the
//! storage backend (DittoStore), providing:
//! - Backend-agnostic API for hierarchical aggregation
//! - Document lifecycle management (create-once, update-many pattern)
//! - Proper separation of business logic from storage operations
//!
//! # Usage
//!
//! ```rust,no_run
//! use hive_protocol::hierarchy::HierarchicalAggregator;
//! use hive_protocol::storage::DittoStore;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create coordinator with DittoStore backend
//! let store = Arc::new(DittoStore::new("app_id".to_string()).await?);
//! let coordinator = HierarchicalAggregator::new(store);
//!
//! // Upsert squad summary (coordinator handles lifecycle)
//! let squad_summary = /* ... */;
//! coordinator.upsert_squad_summary("squad-1A", &squad_summary).await?;
//! # Ok(())
//! # }
//! ```

use crate::storage::DittoStore;
use crate::Result;
use hive_schema::hierarchy::v1::{PlatoonSummary, SquadSummary};
use std::sync::Arc;
use tracing::instrument;

/// Hierarchical Aggregation Coordinator
///
/// Coordinates hierarchical state aggregation operations across the HIVE Protocol,
/// providing a clean separation between application logic and storage backend.
///
/// # Responsibilities
///
/// - Squad summary lifecycle management
/// - Platoon summary lifecycle management
/// - Future: Document versioning and conflict resolution
/// - Future: Bandwidth optimization through delta updates
pub struct HierarchicalAggregator {
    /// Storage backend (DittoStore for now, can be made generic later)
    store: Arc<DittoStore>,
}

impl HierarchicalAggregator {
    /// Create a new HierarchicalAggregator with the given storage backend
    ///
    /// # Arguments
    ///
    /// * `store` - DittoStore instance for persistence operations
    pub fn new(store: Arc<DittoStore>) -> Self {
        Self { store }
    }

    /// Upsert a squad summary document
    ///
    /// This method provides the high-level API for updating squad summaries,
    /// delegating to the storage backend for persistence.
    ///
    /// # Arguments
    ///
    /// * `squad_id` - Unique squad identifier
    /// * `summary` - SquadSummary to persist
    ///
    /// # Returns
    ///
    /// Document ID on success
    ///
    /// # Future Work (ADR-021 Phase 2+)
    ///
    /// - Track document creation vs update to enforce create-once pattern
    /// - Add lifecycle metadata (created_at_us, last_modified_us, version)
    /// - Implement optimistic concurrency control
    #[instrument(skip(self, summary), fields(squad_id))]
    pub async fn upsert_squad_summary(
        &self,
        squad_id: &str,
        summary: &SquadSummary,
    ) -> Result<String> {
        // For now, delegate directly to DittoStore
        // Future: Add lifecycle tracking logic here
        self.store.upsert_squad_summary(squad_id, summary).await
    }

    /// Upsert a platoon summary document
    ///
    /// This method provides the high-level API for updating platoon summaries,
    /// delegating to the storage backend for persistence.
    ///
    /// # Arguments
    ///
    /// * `platoon_id` - Unique platoon identifier
    /// * `summary` - PlatoonSummary to persist
    ///
    /// # Returns
    ///
    /// Document ID on success
    ///
    /// # Future Work (ADR-021 Phase 2+)
    ///
    /// - Track document creation vs update to enforce create-once pattern
    /// - Add lifecycle metadata (created_at_us, last_modified_us, version)
    /// - Implement optimistic concurrency control
    #[instrument(skip(self, summary), fields(platoon_id))]
    pub async fn upsert_platoon_summary(
        &self,
        platoon_id: &str,
        summary: &PlatoonSummary,
    ) -> Result<String> {
        // For now, delegate directly to DittoStore
        // Future: Add lifecycle tracking logic here
        self.store
            .upsert_platoon_summary(platoon_id, summary)
            .await
    }

    /// Get a reference to the underlying storage backend
    ///
    /// This is provided for compatibility during migration.
    /// New code should use the coordinator methods instead.
    pub fn store(&self) -> &Arc<DittoStore> {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_creation() {
        // Coordinator creation is tested in integration tests
        // since it requires Ditto SDK initialization
    }
}
