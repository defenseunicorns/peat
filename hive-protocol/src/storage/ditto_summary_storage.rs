//! Ditto implementation of SummaryStorage trait
//!
//! This module provides the Ditto backend implementation of the backend-agnostic
//! SummaryStorage trait, enabling hierarchical aggregation with Ditto's CRDT engine.

use crate::hierarchy::deltas::{CompanyDelta, PlatoonDelta, SquadDelta};
use crate::hierarchy::storage_trait::{DocumentMetrics, SummaryStorage};
use crate::storage::ditto_store::DittoStore;
use crate::Result;
use async_trait::async_trait;
use hive_schema::hierarchy::v1::{CompanySummary, PlatoonSummary, SquadSummary};
use std::sync::Arc;

/// Ditto-backed implementation of SummaryStorage
///
/// This struct wraps a DittoStore and implements the SummaryStorage trait,
/// providing the Ditto-specific implementation of hierarchical aggregation.
///
/// # Design
///
/// This is a thin wrapper that delegates to DittoStore methods while implementing
/// the backend-agnostic trait. This enables:
/// - Easy backend switching (swap with AutomergeSummaryStorage)
/// - Testing with mock storage implementations
/// - Clean separation between protocol logic and storage backend
pub struct DittoSummaryStorage {
    store: Arc<DittoStore>,
}

impl DittoSummaryStorage {
    /// Create a new DittoSummaryStorage from a DittoStore
    pub fn new(store: Arc<DittoStore>) -> Self {
        Self { store }
    }

    /// Get access to underlying DittoStore (for Ditto-specific operations)
    pub fn store(&self) -> &Arc<DittoStore> {
        &self.store
    }
}

#[async_trait]
impl SummaryStorage for DittoSummaryStorage {
    // ========================================================================
    // Squad Summary Operations
    // ========================================================================

    async fn create_squad_summary(
        &self,
        squad_id: &str,
        initial_state: &SquadSummary,
    ) -> Result<String> {
        self.store
            .create_squad_summary(squad_id, initial_state, None)
            .await
    }

    async fn update_squad_summary(&self, squad_id: &str, delta: SquadDelta) -> Result<()> {
        self.store.update_squad_summary(squad_id, delta, None).await
    }

    async fn get_squad_summary(&self, squad_id: &str) -> Result<Option<SquadSummary>> {
        self.store.get_squad_summary(squad_id, None).await
    }

    async fn delete_squad_summary(&self, _squad_id: &str) -> Result<()> {
        // TODO: Implement delete using DQL DELETE statement
        // For now, deletion is not implemented
        Ok(())
    }

    // ========================================================================
    // Platoon Summary Operations
    // ========================================================================

    async fn create_platoon_summary(
        &self,
        platoon_id: &str,
        initial_state: &PlatoonSummary,
    ) -> Result<String> {
        self.store
            .create_platoon_summary(platoon_id, initial_state, None)
            .await
    }

    async fn update_platoon_summary(&self, platoon_id: &str, delta: PlatoonDelta) -> Result<()> {
        self.store
            .update_platoon_summary(platoon_id, delta, None)
            .await
    }

    async fn get_platoon_summary(&self, platoon_id: &str) -> Result<Option<PlatoonSummary>> {
        self.store.get_platoon_summary(platoon_id, None).await
    }

    async fn delete_platoon_summary(&self, _platoon_id: &str) -> Result<()> {
        // TODO: Implement delete using DQL DELETE statement
        // For now, deletion is not implemented
        Ok(())
    }

    // ========================================================================
    // Company Summary Operations
    // ========================================================================

    async fn create_company_summary(
        &self,
        company_id: &str,
        initial_state: &CompanySummary,
    ) -> Result<String> {
        self.store
            .create_company_summary(company_id, initial_state)
            .await
    }

    async fn update_company_summary(&self, company_id: &str, delta: CompanyDelta) -> Result<()> {
        self.store.update_company_summary(company_id, delta).await
    }

    async fn get_company_summary(&self, company_id: &str) -> Result<Option<CompanySummary>> {
        self.store.get_company_summary(company_id).await
    }

    async fn delete_company_summary(&self, _company_id: &str) -> Result<()> {
        // TODO: Implement delete using DQL DELETE statement
        // For now, deletion is not implemented
        Ok(())
    }

    // ========================================================================
    // Lifecycle Metrics
    // ========================================================================

    async fn get_document_metrics(&self, doc_id: &str) -> Result<DocumentMetrics> {
        self.store.get_document_metrics(doc_id).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_wrapper_creation() {
        // Storage creation is tested in integration tests
        // since it requires Ditto SDK initialization
    }
}
