//! Automerge implementation of SummaryStorage trait
//!
//! This module provides the Automerge backend implementation of the backend-agnostic
//! SummaryStorage trait, enabling hierarchical aggregation with Automerge's CRDT engine.

#[cfg(feature = "automerge-backend")]
use crate::hierarchy::deltas::{CompanyDelta, PlatoonDelta, SquadDelta};
#[cfg(feature = "automerge-backend")]
use crate::hierarchy::storage_trait::{DocumentMetrics, SummaryStorage};
#[cfg(feature = "automerge-backend")]
use crate::hierarchy::SquadFieldUpdate;
#[cfg(feature = "automerge-backend")]
use crate::storage::automerge_conversion::{
    automerge_to_message, automerge_to_message_if_complete, message_to_automerge,
};
#[cfg(feature = "automerge-backend")]
use crate::storage::automerge_store::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use crate::Result;
#[cfg(feature = "automerge-backend")]
use async_trait::async_trait;
#[cfg(feature = "automerge-backend")]
use peat_schema::hierarchy::v1::{CompanySummary, PlatoonSummary, SquadSummary};
#[cfg(feature = "automerge-backend")]
use std::collections::HashMap;
#[cfg(feature = "automerge-backend")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "automerge-backend")]
use std::time::{SystemTime, UNIX_EPOCH};

/// Automerge-backed implementation of SummaryStorage
///
/// This struct wraps an AutomergeStore and implements the SummaryStorage trait,
/// providing the Automerge-specific implementation of hierarchical aggregation.
///
/// # Design
///
/// This is a thin wrapper that delegates to AutomergeStore methods while implementing
/// the backend-agnostic trait. This enables:
/// - Easy backend switching (alternate SummaryStorage implementations)
/// - Testing with mock storage implementations
/// - Clean separation between protocol logic and storage backend
///
/// # Key Naming Convention
///
/// Documents are stored with prefixed keys to separate namespaces:
/// - Squad summaries: `squad-summary:{squad_id}`
/// - Platoon summaries: `platoon-summary:{platoon_id}`
/// - Company summaries: `company-summary:{company_id}`
#[cfg(feature = "automerge-backend")]
pub struct AutomergeSummaryStorage {
    store: Arc<AutomergeStore>,
    /// Metrics tracking for each document (create_count, update_count, etc.)
    metrics: Arc<RwLock<HashMap<String, DocumentMetricsInternal>>>,
}

#[cfg(feature = "automerge-backend")]
struct DocumentMetricsInternal {
    created_at_us: u64,
    create_count: u64,
    update_count: u64,
    last_update_us: u64,
    total_delta_bytes: usize,
}

#[cfg(feature = "automerge-backend")]
impl AutomergeSummaryStorage {
    /// Create a new AutomergeSummaryStorage from an AutomergeStore
    pub fn new(store: Arc<AutomergeStore>) -> Self {
        Self {
            store,
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get access to underlying AutomergeStore (for Automerge-specific operations)
    pub fn store(&self) -> &Arc<AutomergeStore> {
        &self.store
    }

    fn squad_key(squad_id: &str) -> String {
        format!("squad-summary:{}", squad_id)
    }

    fn platoon_key(platoon_id: &str) -> String {
        format!("platoon-summary:{}", platoon_id)
    }

    fn company_key(company_id: &str) -> String {
        format!("company-summary:{}", company_id)
    }

    fn now_us() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    fn record_create(&self, doc_id: &str) {
        let mut metrics = self.metrics.write().unwrap();
        let now = Self::now_us();
        metrics.insert(
            doc_id.to_string(),
            DocumentMetricsInternal {
                created_at_us: now,
                create_count: 1,
                update_count: 0,
                last_update_us: now,
                total_delta_bytes: 0,
            },
        );
    }

    fn record_update(&self, doc_id: &str, delta_bytes: usize) {
        let mut metrics = self.metrics.write().unwrap();
        if let Some(m) = metrics.get_mut(doc_id) {
            m.update_count += 1;
            m.last_update_us = Self::now_us();
            m.total_delta_bytes += delta_bytes;
        }
    }
}

#[cfg(feature = "automerge-backend")]
#[async_trait]
impl SummaryStorage for AutomergeSummaryStorage {
    // ========================================================================
    // Squad Summary Operations
    // ========================================================================

    async fn create_squad_summary(
        &self,
        squad_id: &str,
        initial_state: &SquadSummary,
    ) -> Result<String> {
        let key = Self::squad_key(squad_id);

        // Check if already exists (enforce create-once)
        if self.store.get(&key).ok().flatten().is_some() {
            return Err(crate::Error::storage_error(
                format!("Squad summary {} already exists", squad_id),
                "create_squad_summary",
                Some(key.clone()),
            ));
        }

        // Convert to Automerge document and store
        let doc = message_to_automerge(initial_state).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to convert SquadSummary to Automerge: {}", e),
                "create_squad_summary",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store squad summary: {}", e),
                "create_squad_summary",
                Some(key.clone()),
            )
        })?;

        self.record_create(&key);
        Ok(key)
    }

    async fn update_squad_summary(&self, squad_id: &str, delta: SquadDelta) -> Result<()> {
        let key = Self::squad_key(squad_id);

        // Get existing document
        let doc = self.store.get(&key).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to get squad summary: {}", e),
                "update_squad_summary",
                Some(key.clone()),
            )
        })?;

        let Some(doc) = doc else {
            return Err(crate::Error::storage_error(
                format!("Squad summary {} not found", squad_id),
                "update_squad_summary",
                Some(key.clone()),
            ));
        };

        // Convert to mutable summary, apply delta, convert back
        let mut summary: SquadSummary = automerge_to_message(&doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to deserialize SquadSummary: {}", e),
                "update_squad_summary",
                Some(key.clone()),
            )
        })?;

        // Apply delta fields
        let delta_bytes = apply_squad_delta(&mut summary, delta);

        // Convert back to Automerge and store
        let updated_doc = message_to_automerge(&summary).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to convert updated SquadSummary to Automerge: {}", e),
                "update_squad_summary",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &updated_doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store updated squad summary: {}", e),
                "update_squad_summary",
                Some(key.clone()),
            )
        })?;

        self.record_update(&key, delta_bytes);
        Ok(())
    }

    async fn get_squad_summary(&self, squad_id: &str) -> Result<Option<SquadSummary>> {
        let key = Self::squad_key(squad_id);

        match self.store.get(&key) {
            Ok(Some(doc)) => {
                // Use automerge_to_message_if_complete to handle partial sync gracefully.
                // If "squad_id" field is missing, the document is incomplete - return None.
                let summary = automerge_to_message_if_complete(&doc, "squad_id").map_err(|e| {
                    crate::Error::storage_error(
                        format!("Failed to deserialize SquadSummary: {}", e),
                        "get_squad_summary",
                        Some(key.clone()),
                    )
                })?;
                Ok(summary)
            }
            Ok(None) => Ok(None),
            Err(e) => Err(crate::Error::storage_error(
                format!("Failed to get squad summary: {}", e),
                "get_squad_summary",
                Some(key),
            )),
        }
    }

    async fn delete_squad_summary(&self, squad_id: &str) -> Result<()> {
        let key = Self::squad_key(squad_id);
        self.store.delete(&key).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to delete squad summary: {}", e),
                "delete_squad_summary",
                Some(key.clone()),
            )
        })?;

        // Clean up metrics
        self.metrics.write().unwrap().remove(&key);
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
        let key = Self::platoon_key(platoon_id);

        if self.store.get(&key).ok().flatten().is_some() {
            return Err(crate::Error::storage_error(
                format!("Platoon summary {} already exists", platoon_id),
                "create_platoon_summary",
                Some(key.clone()),
            ));
        }

        let doc = message_to_automerge(initial_state).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to convert PlatoonSummary to Automerge: {}", e),
                "create_platoon_summary",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store platoon summary: {}", e),
                "create_platoon_summary",
                Some(key.clone()),
            )
        })?;

        self.record_create(&key);
        Ok(key)
    }

    async fn update_platoon_summary(&self, platoon_id: &str, delta: PlatoonDelta) -> Result<()> {
        let key = Self::platoon_key(platoon_id);

        let doc = self.store.get(&key).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to get platoon summary: {}", e),
                "update_platoon_summary",
                Some(key.clone()),
            )
        })?;

        let Some(doc) = doc else {
            return Err(crate::Error::storage_error(
                format!("Platoon summary {} not found", platoon_id),
                "update_platoon_summary",
                Some(key.clone()),
            ));
        };

        let mut summary: PlatoonSummary = automerge_to_message(&doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to deserialize PlatoonSummary: {}", e),
                "update_platoon_summary",
                Some(key.clone()),
            )
        })?;

        let delta_bytes = apply_platoon_delta(&mut summary, delta);

        let updated_doc = message_to_automerge(&summary).map_err(|e| {
            crate::Error::storage_error(
                format!(
                    "Failed to convert updated PlatoonSummary to Automerge: {}",
                    e
                ),
                "update_platoon_summary",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &updated_doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store updated platoon summary: {}", e),
                "update_platoon_summary",
                Some(key.clone()),
            )
        })?;

        self.record_update(&key, delta_bytes);
        Ok(())
    }

    async fn get_platoon_summary(&self, platoon_id: &str) -> Result<Option<PlatoonSummary>> {
        let key = Self::platoon_key(platoon_id);

        match self.store.get(&key) {
            Ok(Some(doc)) => {
                // Use automerge_to_message_if_complete to handle partial sync gracefully.
                // If "platoon_id" field is missing, the document is incomplete - return None.
                // This fixes issue #509: Automerge partial sync causes deserialization errors.
                let summary =
                    automerge_to_message_if_complete(&doc, "platoon_id").map_err(|e| {
                        crate::Error::storage_error(
                            format!("Failed to deserialize PlatoonSummary: {}", e),
                            "get_platoon_summary",
                            Some(key.clone()),
                        )
                    })?;
                Ok(summary)
            }
            Ok(None) => Ok(None),
            Err(e) => Err(crate::Error::storage_error(
                format!("Failed to get platoon summary: {}", e),
                "get_platoon_summary",
                Some(key),
            )),
        }
    }

    async fn delete_platoon_summary(&self, platoon_id: &str) -> Result<()> {
        let key = Self::platoon_key(platoon_id);
        self.store.delete(&key).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to delete platoon summary: {}", e),
                "delete_platoon_summary",
                Some(key.clone()),
            )
        })?;

        self.metrics.write().unwrap().remove(&key);
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
        let key = Self::company_key(company_id);

        if self.store.get(&key).ok().flatten().is_some() {
            return Err(crate::Error::storage_error(
                format!("Company summary {} already exists", company_id),
                "create_company_summary",
                Some(key.clone()),
            ));
        }

        let doc = message_to_automerge(initial_state).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to convert CompanySummary to Automerge: {}", e),
                "create_company_summary",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store company summary: {}", e),
                "create_company_summary",
                Some(key.clone()),
            )
        })?;

        self.record_create(&key);
        Ok(key)
    }

    async fn update_company_summary(&self, company_id: &str, delta: CompanyDelta) -> Result<()> {
        let key = Self::company_key(company_id);

        let doc = self.store.get(&key).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to get company summary: {}", e),
                "update_company_summary",
                Some(key.clone()),
            )
        })?;

        let Some(doc) = doc else {
            return Err(crate::Error::storage_error(
                format!("Company summary {} not found", company_id),
                "update_company_summary",
                Some(key.clone()),
            ));
        };

        let mut summary: CompanySummary = automerge_to_message(&doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to deserialize CompanySummary: {}", e),
                "update_company_summary",
                Some(key.clone()),
            )
        })?;

        let delta_bytes = apply_company_delta(&mut summary, delta);

        let updated_doc = message_to_automerge(&summary).map_err(|e| {
            crate::Error::storage_error(
                format!(
                    "Failed to convert updated CompanySummary to Automerge: {}",
                    e
                ),
                "update_company_summary",
                Some(key.clone()),
            )
        })?;

        self.store.put(&key, &updated_doc).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to store updated company summary: {}", e),
                "update_company_summary",
                Some(key.clone()),
            )
        })?;

        self.record_update(&key, delta_bytes);
        Ok(())
    }

    async fn get_company_summary(&self, company_id: &str) -> Result<Option<CompanySummary>> {
        let key = Self::company_key(company_id);

        match self.store.get(&key) {
            Ok(Some(doc)) => {
                // Use automerge_to_message_if_complete to handle partial sync gracefully.
                // If "company_id" field is missing, the document is incomplete - return None.
                let summary =
                    automerge_to_message_if_complete(&doc, "company_id").map_err(|e| {
                        crate::Error::storage_error(
                            format!("Failed to deserialize CompanySummary: {}", e),
                            "get_company_summary",
                            Some(key.clone()),
                        )
                    })?;
                Ok(summary)
            }
            Ok(None) => Ok(None),
            Err(e) => Err(crate::Error::storage_error(
                format!("Failed to get company summary: {}", e),
                "get_company_summary",
                Some(key),
            )),
        }
    }

    async fn delete_company_summary(&self, company_id: &str) -> Result<()> {
        let key = Self::company_key(company_id);
        self.store.delete(&key).map_err(|e| {
            crate::Error::storage_error(
                format!("Failed to delete company summary: {}", e),
                "delete_company_summary",
                Some(key.clone()),
            )
        })?;

        self.metrics.write().unwrap().remove(&key);
        Ok(())
    }

    // ========================================================================
    // Lifecycle Metrics
    // ========================================================================

    async fn get_document_metrics(&self, doc_id: &str) -> Result<DocumentMetrics> {
        let metrics = self.metrics.read().unwrap();

        let internal = metrics.get(doc_id).ok_or_else(|| {
            crate::Error::storage_error(
                format!("No metrics found for document {}", doc_id),
                "get_document_metrics",
                Some(doc_id.to_string()),
            )
        })?;

        // Get current document size
        let full_doc_size = self
            .store
            .get(doc_id)
            .ok()
            .flatten()
            .map(|doc| doc.save().len())
            .unwrap_or(0);

        let avg_delta_size = if internal.update_count > 0 {
            internal.total_delta_bytes / internal.update_count as usize
        } else {
            0
        };

        let compression_ratio = if avg_delta_size > 0 {
            full_doc_size as f32 / avg_delta_size as f32
        } else {
            0.0
        };

        Ok(DocumentMetrics {
            document_id: doc_id.to_string(),
            created_at_us: internal.created_at_us,
            create_count: internal.create_count,
            update_count: internal.update_count,
            last_update_us: internal.last_update_us,
            total_delta_bytes: internal.total_delta_bytes,
            full_doc_size,
            compression_ratio,
            sequence: internal.update_count,
        })
    }
}

// ============================================================================
// Delta Application Helpers
// ============================================================================

#[cfg(feature = "automerge-backend")]
use crate::hierarchy::deltas::{CompanyFieldUpdate, PlatoonFieldUpdate};

/// Apply squad delta to summary, returns approximate delta size in bytes
#[cfg(feature = "automerge-backend")]
fn apply_squad_delta(summary: &mut SquadSummary, delta: SquadDelta) -> usize {
    let mut bytes = 0;

    for update in delta.updates {
        match update {
            SquadFieldUpdate::SetLeaderId(id) => {
                summary.leader_id = id;
                bytes += 16;
            }
            SquadFieldUpdate::SetMemberCount(count) => {
                summary.member_count = count;
                bytes += 4;
            }
            SquadFieldUpdate::SetOperationalCount(count) => {
                summary.operational_count = count;
                bytes += 4;
            }
            SquadFieldUpdate::SetAvgFuelMinutes(fuel) => {
                summary.avg_fuel_minutes = fuel;
                bytes += 4;
            }
            SquadFieldUpdate::SetWorstHealth(health) => {
                summary.worst_health = health;
                bytes += 4;
            }
            SquadFieldUpdate::SetReadinessScore(score) => {
                summary.readiness_score = score;
                bytes += 4;
            }
            SquadFieldUpdate::UpdatePositionCentroid(pos) => {
                summary.position_centroid = Some(pos);
                bytes += 24;
            }
            SquadFieldUpdate::AddMemberId(id) => {
                if !summary.member_ids.contains(&id) {
                    summary.member_ids.push(id);
                }
                bytes += 16;
            }
            SquadFieldUpdate::RemoveMemberId(id) => {
                summary.member_ids.retain(|m| m != &id);
                bytes += 8;
            }
            SquadFieldUpdate::AddCapability(cap) => {
                summary.aggregated_capabilities.push(cap);
                bytes += 100;
            }
            SquadFieldUpdate::RemoveCapability(cap_id) => {
                summary.aggregated_capabilities.retain(|c| c.id != cap_id);
                bytes += 8;
            }
            SquadFieldUpdate::UpdateBoundingBox(bbox) => {
                summary.bounding_box = Some(bbox);
                bytes += 40;
            }
            SquadFieldUpdate::UpdateAggregatedAt(ts) => {
                summary.aggregated_at = Some(ts);
                bytes += 16;
            }
        }
    }

    bytes
}

/// Apply platoon delta to summary, returns approximate delta size in bytes
#[cfg(feature = "automerge-backend")]
fn apply_platoon_delta(summary: &mut PlatoonSummary, delta: PlatoonDelta) -> usize {
    let mut bytes = 0;

    for update in delta.updates {
        match update {
            PlatoonFieldUpdate::SetLeaderId(id) => {
                summary.leader_id = id;
                bytes += 16;
            }
            PlatoonFieldUpdate::SetSquadCount(count) => {
                summary.squad_count = count;
                bytes += 4;
            }
            PlatoonFieldUpdate::SetTotalMemberCount(count) => {
                summary.total_member_count = count;
                bytes += 4;
            }
            PlatoonFieldUpdate::SetOperationalCount(count) => {
                summary.operational_count = count;
                bytes += 4;
            }
            PlatoonFieldUpdate::SetAvgFuelMinutes(fuel) => {
                summary.avg_fuel_minutes = fuel;
                bytes += 4;
            }
            PlatoonFieldUpdate::SetWorstHealth(health) => {
                summary.worst_health = health;
                bytes += 4;
            }
            PlatoonFieldUpdate::SetReadinessScore(score) => {
                summary.readiness_score = score;
                bytes += 4;
            }
            PlatoonFieldUpdate::UpdatePositionCentroid(pos) => {
                summary.position_centroid = Some(pos);
                bytes += 24;
            }
            PlatoonFieldUpdate::AddSquadId(id) => {
                if !summary.squad_ids.contains(&id) {
                    summary.squad_ids.push(id);
                }
                bytes += 16;
            }
            PlatoonFieldUpdate::RemoveSquadId(id) => {
                summary.squad_ids.retain(|s| s != &id);
                bytes += 8;
            }
            PlatoonFieldUpdate::AddCapability(cap) => {
                summary.aggregated_capabilities.push(cap);
                bytes += 100;
            }
            PlatoonFieldUpdate::RemoveCapability(cap_id) => {
                summary.aggregated_capabilities.retain(|c| c.id != cap_id);
                bytes += 8;
            }
            PlatoonFieldUpdate::UpdateBoundingBox(bbox) => {
                summary.bounding_box = Some(bbox);
                bytes += 40;
            }
            PlatoonFieldUpdate::UpdateAggregatedAt(ts) => {
                summary.aggregated_at = Some(ts);
                bytes += 16;
            }
        }
    }

    bytes
}

/// Apply company delta to summary, returns approximate delta size in bytes
#[cfg(feature = "automerge-backend")]
fn apply_company_delta(summary: &mut CompanySummary, delta: CompanyDelta) -> usize {
    let mut bytes = 0;

    for update in delta.updates {
        match update {
            CompanyFieldUpdate::SetLeaderId(id) => {
                summary.leader_id = id;
                bytes += 16;
            }
            CompanyFieldUpdate::SetPlatoonCount(count) => {
                summary.platoon_count = count;
                bytes += 4;
            }
            CompanyFieldUpdate::SetTotalMemberCount(count) => {
                summary.total_member_count = count;
                bytes += 4;
            }
            CompanyFieldUpdate::SetOperationalCount(count) => {
                summary.operational_count = count;
                bytes += 4;
            }
            CompanyFieldUpdate::SetAvgFuelMinutes(fuel) => {
                summary.avg_fuel_minutes = fuel;
                bytes += 4;
            }
            CompanyFieldUpdate::SetWorstHealth(health) => {
                summary.worst_health = health;
                bytes += 4;
            }
            CompanyFieldUpdate::SetReadinessScore(score) => {
                summary.readiness_score = score;
                bytes += 4;
            }
            CompanyFieldUpdate::UpdatePositionCentroid(pos) => {
                summary.position_centroid = Some(pos);
                bytes += 24;
            }
            CompanyFieldUpdate::AddPlatoonId(id) => {
                if !summary.platoon_ids.contains(&id) {
                    summary.platoon_ids.push(id);
                }
                bytes += 16;
            }
            CompanyFieldUpdate::RemovePlatoonId(id) => {
                summary.platoon_ids.retain(|p| p != &id);
                bytes += 8;
            }
            CompanyFieldUpdate::AddCapability(cap) => {
                summary.aggregated_capabilities.push(cap);
                bytes += 100;
            }
            CompanyFieldUpdate::RemoveCapability(cap_id) => {
                summary.aggregated_capabilities.retain(|c| c.id != cap_id);
                bytes += 8;
            }
            CompanyFieldUpdate::UpdateBoundingBox(bbox) => {
                summary.bounding_box = Some(bbox);
                bytes += 40;
            }
            CompanyFieldUpdate::UpdateAggregatedAt(ts) => {
                summary.aggregated_at = Some(ts);
                bytes += 16;
            }
        }
    }

    bytes
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use peat_schema::common::v1::{Position, Timestamp};
    use tempfile::TempDir;

    fn create_test_storage() -> (AutomergeSummaryStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
        (AutomergeSummaryStorage::new(store), temp_dir)
    }

    #[tokio::test]
    async fn test_squad_summary_crud() {
        let (storage, _temp) = create_test_storage();

        // Create
        let summary = SquadSummary {
            squad_id: "squad-1".to_string(),
            leader_id: "leader-1".to_string(),
            member_ids: vec!["m1".to_string(), "m2".to_string()],
            member_count: 2,
            position_centroid: Some(Position {
                latitude: 37.0,
                longitude: -122.0,
                altitude: 100.0,
            }),
            avg_fuel_minutes: 60.0,
            worst_health: 0,
            operational_count: 2,
            aggregated_capabilities: vec![],
            readiness_score: 0.9,
            bounding_box: None,
            aggregated_at: Some(Timestamp {
                seconds: 1234567890,
                nanos: 0,
            }),
        };

        let doc_id = storage
            .create_squad_summary("squad-1", &summary)
            .await
            .expect("create should succeed");
        assert!(doc_id.contains("squad-1"));

        // Read
        let retrieved = storage
            .get_squad_summary("squad-1")
            .await
            .expect("get should succeed")
            .expect("summary should exist");
        assert_eq!(retrieved.squad_id, "squad-1");
        assert_eq!(retrieved.member_count, 2);

        // Update
        let delta = SquadDelta {
            squad_id: "squad-1".to_string(),
            timestamp_us: 0,
            sequence: 1,
            updates: vec![
                SquadFieldUpdate::SetAvgFuelMinutes(50.0),
                SquadFieldUpdate::SetOperationalCount(1),
            ],
        };
        storage
            .update_squad_summary("squad-1", delta)
            .await
            .expect("update should succeed");

        let updated = storage.get_squad_summary("squad-1").await.unwrap().unwrap();
        assert_eq!(updated.avg_fuel_minutes, 50.0);
        assert_eq!(updated.operational_count, 1);

        // Delete
        storage
            .delete_squad_summary("squad-1")
            .await
            .expect("delete should succeed");
        assert!(storage
            .get_squad_summary("squad-1")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_create_once_enforcement() {
        let (storage, _temp) = create_test_storage();

        let summary = SquadSummary {
            squad_id: "squad-1".to_string(),
            ..Default::default()
        };

        // First create should succeed
        storage
            .create_squad_summary("squad-1", &summary)
            .await
            .expect("first create should succeed");

        // Second create should fail (create-once enforcement)
        let result = storage.create_squad_summary("squad-1", &summary).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_document_metrics() {
        let (storage, _temp) = create_test_storage();

        let summary = SquadSummary {
            squad_id: "squad-1".to_string(),
            avg_fuel_minutes: 60.0,
            ..Default::default()
        };

        let doc_id = storage
            .create_squad_summary("squad-1", &summary)
            .await
            .unwrap();

        // Apply some updates
        for i in 0..5 {
            let delta = SquadDelta {
                squad_id: "squad-1".to_string(),
                timestamp_us: crate::hierarchy::deltas::current_timestamp_us(),
                sequence: i + 1,
                updates: vec![SquadFieldUpdate::SetAvgFuelMinutes(55.0)],
            };
            storage
                .update_squad_summary("squad-1", delta)
                .await
                .unwrap();
        }

        let metrics = storage.get_document_metrics(&doc_id).await.unwrap();
        assert_eq!(metrics.create_count, 1);
        assert_eq!(metrics.update_count, 5);
        assert!(metrics.total_delta_bytes > 0);
    }
}
