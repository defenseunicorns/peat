//! Hierarchical Aggregation Coordinator
//!
//! This module provides the coordinator layer for hierarchical state aggregation,
//! implementing ADR-021 document-oriented architecture principles.
//!
//! # Architecture
//!
//! The HierarchicalAggregator sits between the application (hive-sim) and the
//! storage backend, providing:
//! - Backend-agnostic API for hierarchical aggregation
//! - Document lifecycle management (create-once, update-many pattern)
//! - Proper separation of business logic from storage operations
//!
//! # Usage
//!
//! ```rust,ignore
//! use hive_protocol::hierarchy::{HierarchicalAggregator, storage_trait::SummaryStorage};
//! use std::sync::Arc;
//!
//! # async fn example(storage: Arc<dyn SummaryStorage>) -> Result<(), Box<dyn std::error::Error>> {
//! // Create coordinator with any SummaryStorage backend (Ditto, Automerge, etc.)
//! let coordinator = HierarchicalAggregator::new(storage);
//!
//! // Create squad summary once
//! let squad_summary = /* ... */;
//! coordinator.create_squad_summary("squad-1A", &squad_summary).await?;
//!
//! // Update with deltas
//! let delta = /* ... */;
//! coordinator.update_squad_summary("squad-1A", delta).await?;
//! # Ok(())
//! # }
//! ```

use crate::hierarchy::deltas::{CompanyDelta, PlatoonDelta, SquadDelta};
use crate::hierarchy::storage_trait::{DocumentMetrics, SummaryStorage};
use crate::Result;
use hive_schema::hierarchy::v1::{CompanySummary, PlatoonSummary, SquadSummary};
use std::sync::Arc;
use tracing::instrument;

/// Hierarchical Aggregation Coordinator
///
/// Coordinates hierarchical state aggregation operations across the HIVE Protocol,
/// providing a clean separation between application logic and storage backend.
///
/// # Design
///
/// This coordinator is backend-agnostic - it works with any implementation of
/// `SummaryStorage` trait (Ditto, Automerge/Iroh, etc.). This enables:
/// - Easy backend switching without changing application code
/// - Testing with mock storage implementations
/// - Future optimization opportunities (caching, batching, etc.)
///
/// # Responsibilities
///
/// - Squad/Platoon/Company summary lifecycle management
/// - Delta computation from state changes
/// - Lifecycle metrics validation
/// - Future: Document versioning and conflict resolution
pub struct HierarchicalAggregator {
    /// Storage backend (trait object for backend flexibility)
    storage: Arc<dyn SummaryStorage>,
}

impl HierarchicalAggregator {
    /// Create a new HierarchicalAggregator with the given storage backend
    ///
    /// # Arguments
    ///
    /// * `storage` - Any implementation of SummaryStorage trait
    pub fn new(storage: Arc<dyn SummaryStorage>) -> Self {
        Self { storage }
    }

    // ========================================================================
    // Squad Summary Operations
    // ========================================================================

    /// Create a squad summary document (called ONCE during squad formation)
    #[instrument(skip(self, initial_state), fields(squad_id))]
    pub async fn create_squad_summary(
        &self,
        squad_id: &str,
        initial_state: &SquadSummary,
    ) -> Result<String> {
        self.storage
            .create_squad_summary(squad_id, initial_state)
            .await
    }

    /// Update squad summary with delta (called MANY times)
    #[instrument(skip(self, delta), fields(squad_id))]
    pub async fn update_squad_summary(&self, squad_id: &str, delta: SquadDelta) -> Result<()> {
        self.storage.update_squad_summary(squad_id, delta).await
    }

    /// Retrieve squad summary
    #[instrument(skip(self), fields(squad_id))]
    pub async fn get_squad_summary(&self, squad_id: &str) -> Result<Option<SquadSummary>> {
        self.storage.get_squad_summary(squad_id).await
    }

    // ========================================================================
    // Platoon Summary Operations
    // ========================================================================

    /// Create a platoon summary document (called ONCE during platoon formation)
    #[instrument(skip(self, initial_state), fields(platoon_id))]
    pub async fn create_platoon_summary(
        &self,
        platoon_id: &str,
        initial_state: &PlatoonSummary,
    ) -> Result<String> {
        self.storage
            .create_platoon_summary(platoon_id, initial_state)
            .await
    }

    /// Update platoon summary with delta (called MANY times)
    #[instrument(skip(self, delta), fields(platoon_id))]
    pub async fn update_platoon_summary(
        &self,
        platoon_id: &str,
        delta: PlatoonDelta,
    ) -> Result<()> {
        self.storage.update_platoon_summary(platoon_id, delta).await
    }

    /// Retrieve platoon summary
    #[instrument(skip(self), fields(platoon_id))]
    pub async fn get_platoon_summary(&self, platoon_id: &str) -> Result<Option<PlatoonSummary>> {
        self.storage.get_platoon_summary(platoon_id).await
    }

    // ========================================================================
    // Company Summary Operations
    // ========================================================================

    /// Create a company summary document (called ONCE during company formation)
    #[instrument(skip(self, initial_state), fields(company_id))]
    pub async fn create_company_summary(
        &self,
        company_id: &str,
        initial_state: &CompanySummary,
    ) -> Result<String> {
        self.storage
            .create_company_summary(company_id, initial_state)
            .await
    }

    /// Update company summary with delta (called MANY times)
    #[instrument(skip(self, delta), fields(company_id))]
    pub async fn update_company_summary(
        &self,
        company_id: &str,
        delta: CompanyDelta,
    ) -> Result<()> {
        self.storage.update_company_summary(company_id, delta).await
    }

    /// Retrieve company summary
    #[instrument(skip(self), fields(company_id))]
    pub async fn get_company_summary(&self, company_id: &str) -> Result<Option<CompanySummary>> {
        self.storage.get_company_summary(company_id).await
    }

    // ========================================================================
    // Lifecycle Metrics (for validation)
    // ========================================================================

    /// Get document lifecycle metrics for validation
    ///
    /// Returns metrics for validating ADR-021 architectural invariants.
    #[instrument(skip(self), fields(doc_id))]
    pub async fn get_document_metrics(&self, doc_id: &str) -> Result<DocumentMetrics> {
        self.storage.get_document_metrics(doc_id).await
    }

    /// Validate document lifecycle invariants
    ///
    /// Checks that:
    /// - Document created exactly once (create_count == 1)
    /// - Delta efficiency is good (compression_ratio > 10×)
    #[instrument(skip(self), fields(doc_id))]
    pub async fn validate_document(&self, doc_id: &str) -> Result<()> {
        let metrics = self.get_document_metrics(doc_id).await?;
        metrics.validate()
    }

    // ========================================================================
    // Backward Compatibility Methods (DEPRECATED - use create/update instead)
    // ========================================================================

    /// Upsert a squad summary (DEPRECATED - use create_squad_summary + update_squad_summary)
    ///
    /// This method exists for backward compatibility with existing code.
    /// New code should use create_squad_summary() once, then update_squad_summary() many times.
    #[deprecated(note = "Use create_squad_summary() once, then update_squad_summary() for updates")]
    #[instrument(skip(self, summary), fields(squad_id))]
    pub async fn upsert_squad_summary(
        &self,
        squad_id: &str,
        summary: &SquadSummary,
    ) -> Result<String> {
        // Try to get existing document
        match self.get_squad_summary(squad_id).await? {
            Some(_existing) => {
                // Document exists - this would require creating a delta
                // For now, just return the doc ID
                // TODO: In the future, compute delta and call update
                Ok(format!("{}-summary", squad_id))
            }
            None => {
                // Document doesn't exist - create it
                self.create_squad_summary(squad_id, summary).await
            }
        }
    }

    /// Upsert a platoon summary (DEPRECATED - use create_platoon_summary + update_platoon_summary)
    #[deprecated(
        note = "Use create_platoon_summary() once, then update_platoon_summary() for updates"
    )]
    #[instrument(skip(self, summary), fields(platoon_id))]
    pub async fn upsert_platoon_summary(
        &self,
        platoon_id: &str,
        summary: &PlatoonSummary,
    ) -> Result<String> {
        match self.get_platoon_summary(platoon_id).await? {
            Some(_existing) => Ok(format!("{}-summary", platoon_id)),
            None => self.create_platoon_summary(platoon_id, summary).await,
        }
    }

    /// Get reference to underlying storage (for backend-specific operations)
    ///
    /// This is intentionally not part of the public API to maintain backend abstraction,
    /// but is needed for some legacy code paths.
    #[doc(hidden)]
    pub fn storage(&self) -> &Arc<dyn SummaryStorage> {
        &self.storage
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_coordinator_creation() {
        // Coordinator creation is tested in integration tests
        // since it requires Ditto SDK initialization
    }
}
