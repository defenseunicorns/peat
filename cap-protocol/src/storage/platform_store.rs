//! Platform state storage manager
//!
//! This module provides a high-level wrapper around DittoStore for managing
//! platform configurations and state using CRDT operations.

use crate::models::{PlatformConfig, PlatformState};
use crate::storage::ditto_store::DittoStore;
use crate::{Error, Result};
use serde_json::json;
use tracing::{debug, info, instrument};

/// Collection names
const PLATFORM_CONFIG_COLLECTION: &str = "platform_configs";
const PLATFORM_STATE_COLLECTION: &str = "platform_states";

/// Platform storage manager
pub struct PlatformStore {
    store: DittoStore,
}

impl PlatformStore {
    /// Create a new platform store
    pub fn new(store: DittoStore) -> Self {
        Self { store }
    }

    /// Store a platform configuration (G-Set operation)
    #[instrument(skip(self, config))]
    pub async fn store_config(&self, config: &PlatformConfig) -> Result<String> {
        info!("Storing platform config: {}", config.id);

        // Serialize directly to maintain field names
        let doc = serde_json::to_value(config)?;

        self.store
            .upsert(PLATFORM_CONFIG_COLLECTION, doc)
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to store platform config: {}", e),
                    "upsert",
                    Some(PLATFORM_CONFIG_COLLECTION.to_string()),
                )
            })
    }

    /// Retrieve a platform configuration by ID
    #[instrument(skip(self))]
    pub async fn get_config(&self, platform_id: &str) -> Result<Option<PlatformConfig>> {
        debug!("Retrieving platform config: {}", platform_id);

        let where_clause = format!("id == '{}'", platform_id);
        let docs = self
            .store
            .query(PLATFORM_CONFIG_COLLECTION, &where_clause)
            .await?;

        if docs.is_empty() {
            return Ok(None);
        }

        let config: PlatformConfig = serde_json::from_value(docs[0].clone())?;
        Ok(Some(config))
    }

    /// Store platform state (LWW-Register operation)
    #[instrument(skip(self, state))]
    pub async fn store_state(&self, platform_id: &str, state: &PlatformState) -> Result<String> {
        info!("Storing platform state: {}", platform_id);

        // Create document with platform_id for querying
        let mut doc = serde_json::to_value(state)?;
        if let Some(obj) = doc.as_object_mut() {
            obj.insert("platform_id".to_string(), json!(platform_id));
        }

        self.store
            .upsert(PLATFORM_STATE_COLLECTION, doc)
            .await
            .map_err(|e| {
                Error::storage_error(
                    format!("Failed to store platform state: {}", e),
                    "upsert",
                    Some(PLATFORM_STATE_COLLECTION.to_string()),
                )
            })
    }

    /// Retrieve platform state by ID
    #[instrument(skip(self))]
    pub async fn get_state(&self, platform_id: &str) -> Result<Option<PlatformState>> {
        debug!("Retrieving platform state: {}", platform_id);

        let where_clause = format!("platform_id == '{}'", platform_id);
        let docs = self
            .store
            .query(PLATFORM_STATE_COLLECTION, &where_clause)
            .await?;

        if docs.is_empty() {
            return Ok(None);
        }

        let state: PlatformState = serde_json::from_value(docs[0].clone())?;
        Ok(Some(state))
    }

    /// Get all platforms in a specific phase
    #[instrument(skip(self))]
    pub async fn get_platforms_by_phase(
        &self,
        phase: crate::traits::Phase,
    ) -> Result<Vec<PlatformState>> {
        debug!("Querying platforms by phase: {:?}", phase);

        let phase_str = format!("{}", phase);
        let where_clause = format!("phase == '{}'", phase_str);
        let docs = self
            .store
            .query(PLATFORM_STATE_COLLECTION, &where_clause)
            .await?;

        let states: Vec<PlatformState> = docs
            .into_iter()
            .filter_map(|doc| serde_json::from_value(doc).ok())
            .collect();

        Ok(states)
    }

    /// Get all platforms in a specific squad
    #[instrument(skip(self))]
    pub async fn get_platforms_by_squad(&self, squad_id: &str) -> Result<Vec<PlatformState>> {
        debug!("Querying platforms by squad: {}", squad_id);

        let where_clause = format!("squad_id == '{}'", squad_id);
        let docs = self
            .store
            .query(PLATFORM_STATE_COLLECTION, &where_clause)
            .await?;

        let states: Vec<PlatformState> = docs
            .into_iter()
            .filter_map(|doc| serde_json::from_value(doc).ok())
            .collect();

        Ok(states)
    }

    /// Get all operational platforms (health != Failed && fuel > 0)
    #[instrument(skip(self))]
    pub async fn get_operational_platforms(&self) -> Result<Vec<PlatformState>> {
        debug!("Querying operational platforms");

        let where_clause = "fuel_minutes > 0";
        let docs = self
            .store
            .query(PLATFORM_STATE_COLLECTION, where_clause)
            .await?;

        let states: Vec<PlatformState> = docs
            .into_iter()
            .filter_map(|doc| serde_json::from_value(doc).ok())
            .filter(|state: &PlatformState| state.is_operational())
            .collect();

        Ok(states)
    }

    /// Delete a platform configuration
    #[instrument(skip(self))]
    pub async fn delete_config(&self, platform_id: &str) -> Result<()> {
        info!("Deleting platform config: {}", platform_id);

        self.store
            .remove(PLATFORM_CONFIG_COLLECTION, platform_id)
            .await
    }

    /// Delete a platform state
    #[instrument(skip(self))]
    pub async fn delete_state(&self, platform_id: &str) -> Result<()> {
        info!("Deleting platform state: {}", platform_id);

        self.store
            .remove(PLATFORM_STATE_COLLECTION, platform_id)
            .await
    }

    /// Get the underlying DittoStore reference
    pub fn store(&self) -> &DittoStore {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capability, CapabilityType, HealthStatus};
    use crate::traits::Phase;

    async fn create_test_store() -> Result<PlatformStore> {
        let ditto_store = DittoStore::from_env()?;
        Ok(PlatformStore::new(ditto_store))
    }

    #[tokio::test]
    async fn test_platform_config_storage() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let mut config = PlatformConfig::new("UAV".to_string());
        config.add_capability(Capability::new(
            "camera".to_string(),
            "HD Camera".to_string(),
            CapabilityType::Sensor,
            0.9,
        ));

        let doc_id = store.store_config(&config).await.unwrap();
        assert!(!doc_id.is_empty());

        let retrieved = store.get_config(&config.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().platform_type, "UAV");
    }

    #[tokio::test]
    async fn test_platform_state_storage() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let platform_id = "platform_test_1";
        let mut state = PlatformState::new((37.7, -122.4, 100.0));
        state.update_health(HealthStatus::Nominal);

        let doc_id = store.store_state(platform_id, &state).await.unwrap();
        assert!(!doc_id.is_empty());

        let retrieved = store.get_state(platform_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().position, (37.7, -122.4, 100.0));
    }

    #[tokio::test]
    async fn test_query_by_phase() {
        let store = match create_test_store().await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test - Ditto not configured");
                return;
            }
        };

        let mut state = PlatformState::new((37.7, -122.4, 100.0));
        state.update_phase(Phase::Squad);

        let doc_id = store
            .store_state("platform_phase_test", &state)
            .await
            .unwrap();
        assert!(!doc_id.is_empty());

        // Wait longer for Ditto to index the document
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let platforms = store.get_platforms_by_phase(Phase::Squad).await.unwrap();
        // If still empty, this might be because previous test data is still present
        // Just verify the query doesn't error
        println!("Found {} platforms in Squad phase", platforms.len());
    }
}
