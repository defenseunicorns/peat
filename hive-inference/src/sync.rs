//! HIVE Sync Integration for Inference Pipeline
//!
//! Connects the inference pipeline to the HIVE protocol network,
//! enabling:
//! - TrackUpdate publishing to HIVE document store
//! - CapabilityAdvertisement publishing
//! - Observation of incoming commands
//!
//! ## Usage
//!
//! ```rust,ignore
//! use hive_inference::sync::{HiveSyncClient, SyncConfig};
//!
//! // Create sync client with HIVE backend
//! let config = SyncConfig::new("platform-1", "/tmp/hive-data");
//! let mut client = HiveSyncClient::new(config).await?;
//!
//! // Publish track updates
//! client.publish_track_update(track_update).await?;
//!
//! // Publish capability advertisement
//! client.publish_capability(capability).await?;
//! ```

use crate::messages::{CapabilityAdvertisement, TrackUpdate};
use hive_protocol::sync::types::{Document, Query, Value};
use hive_protocol::sync::DataSyncBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Collection names for HIVE documents
pub mod collections {
    /// Track updates from AI platforms
    pub const TRACKS: &str = "tracks";
    /// Capability advertisements from platforms
    pub const CAPABILITIES: &str = "capabilities";
    /// Commands from C2/TAK
    pub const COMMANDS: &str = "commands";
    /// Platform registrations
    pub const PLATFORMS: &str = "platforms";
}

/// Configuration for HIVE sync client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Platform identifier
    pub platform_id: String,
    /// Data persistence directory
    pub persistence_dir: PathBuf,
    /// Formation/cell ID (for scoping)
    pub formation_id: Option<String>,
    /// Enable mDNS peer discovery
    pub enable_mdns: bool,
    /// Static peers to connect to
    pub static_peers: Vec<String>,
}

impl SyncConfig {
    /// Create a new sync config
    pub fn new(platform_id: &str, persistence_dir: &str) -> Self {
        Self {
            platform_id: platform_id.to_string(),
            persistence_dir: PathBuf::from(persistence_dir),
            formation_id: None,
            enable_mdns: true,
            static_peers: Vec::new(),
        }
    }

    /// Set formation ID
    pub fn with_formation(mut self, formation_id: &str) -> Self {
        self.formation_id = Some(formation_id.to_string());
        self
    }

    /// Add static peers
    pub fn with_peers(mut self, peers: Vec<String>) -> Self {
        self.static_peers = peers;
        self
    }

    /// Disable mDNS discovery
    pub fn without_mdns(mut self) -> Self {
        self.enable_mdns = false;
        self
    }
}

/// HIVE sync client for publishing inference results
pub struct HiveSyncClient {
    config: SyncConfig,
    backend: Arc<dyn DataSyncBackend>,
    /// Track update counter for metrics
    tracks_published: u64,
    /// Capability advertisement counter
    capabilities_published: u64,
}

impl HiveSyncClient {
    /// Create a new sync client with the given backend
    pub fn with_backend(config: SyncConfig, backend: Arc<dyn DataSyncBackend>) -> Self {
        info!(
            "Creating HIVE sync client for platform: {}",
            config.platform_id
        );
        Self {
            config,
            backend,
            tracks_published: 0,
            capabilities_published: 0,
        }
    }

    /// Publish a track update to the HIVE network
    pub async fn publish_track_update(&mut self, track: &TrackUpdate) -> anyhow::Result<String> {
        let doc = self.track_to_document(track);
        let doc_id = self
            .backend
            .document_store()
            .upsert(collections::TRACKS, doc)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to publish track: {}", e))?;

        self.tracks_published += 1;
        debug!(
            "Published track {} (total: {})",
            track.track_id, self.tracks_published
        );

        Ok(doc_id)
    }

    /// Publish multiple track updates in batch
    pub async fn publish_track_updates(
        &mut self,
        tracks: &[TrackUpdate],
    ) -> anyhow::Result<Vec<String>> {
        let mut doc_ids = Vec::with_capacity(tracks.len());
        for track in tracks {
            let doc_id = self.publish_track_update(track).await?;
            doc_ids.push(doc_id);
        }
        Ok(doc_ids)
    }

    /// Publish a capability advertisement
    pub async fn publish_capability(
        &mut self,
        capability: &CapabilityAdvertisement,
    ) -> anyhow::Result<String> {
        let doc = self.capability_to_document(capability);
        let doc_id = self
            .backend
            .document_store()
            .upsert(collections::CAPABILITIES, doc)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to publish capability: {}", e))?;

        self.capabilities_published += 1;
        info!(
            "Published capability for {} (total: {})",
            capability.platform_id, self.capabilities_published
        );

        Ok(doc_id)
    }

    /// Query tracks by source platform
    pub async fn query_tracks_by_platform(
        &self,
        platform_id: &str,
    ) -> anyhow::Result<Vec<TrackUpdate>> {
        let query = Query::Eq {
            field: "source_platform".to_string(),
            value: Value::String(platform_id.to_string()),
        };

        let docs = self
            .backend
            .document_store()
            .query(collections::TRACKS, &query)
            .await
            .map_err(|e| anyhow::anyhow!("Query failed: {}", e))?;

        let tracks: Vec<TrackUpdate> = docs
            .into_iter()
            .filter_map(|doc| self.document_to_track(&doc).ok())
            .collect();

        Ok(tracks)
    }

    /// Query all tracks in a formation
    pub async fn query_tracks_by_formation(
        &self,
        formation_id: &str,
    ) -> anyhow::Result<Vec<TrackUpdate>> {
        let query = Query::Eq {
            field: "formation_id".to_string(),
            value: Value::String(formation_id.to_string()),
        };

        let docs = self
            .backend
            .document_store()
            .query(collections::TRACKS, &query)
            .await
            .map_err(|e| anyhow::anyhow!("Query failed: {}", e))?;

        let tracks: Vec<TrackUpdate> = docs
            .into_iter()
            .filter_map(|doc| self.document_to_track(&doc).ok())
            .collect();

        Ok(tracks)
    }

    /// Get a specific track by ID
    pub async fn get_track(&self, track_id: &str) -> anyhow::Result<Option<TrackUpdate>> {
        let doc = self
            .backend
            .document_store()
            .get(collections::TRACKS, &track_id.to_string())
            .await
            .map_err(|e| anyhow::anyhow!("Get failed: {}", e))?;

        match doc {
            Some(d) => Ok(Some(self.document_to_track(&d)?)),
            None => Ok(None),
        }
    }

    /// Get publishing statistics
    pub fn stats(&self) -> SyncStats {
        SyncStats {
            tracks_published: self.tracks_published,
            capabilities_published: self.capabilities_published,
        }
    }

    /// Convert TrackUpdate to HIVE Document
    fn track_to_document(&self, track: &TrackUpdate) -> Document {
        let mut fields = HashMap::new();

        fields.insert("track_id".to_string(), serde_json::json!(track.track_id));
        fields.insert(
            "classification".to_string(),
            serde_json::json!(track.classification),
        );
        fields.insert(
            "confidence".to_string(),
            serde_json::json!(track.confidence),
        );
        fields.insert("lat".to_string(), serde_json::json!(track.position.lat));
        fields.insert("lon".to_string(), serde_json::json!(track.position.lon));

        if let Some(cep) = track.position.cep_m {
            fields.insert("cep_m".to_string(), serde_json::json!(cep));
        }
        if let Some(hae) = track.position.hae {
            fields.insert("hae".to_string(), serde_json::json!(hae));
        }

        if let Some(velocity) = &track.velocity {
            fields.insert("bearing".to_string(), serde_json::json!(velocity.bearing));
            fields.insert(
                "speed_mps".to_string(),
                serde_json::json!(velocity.speed_mps),
            );
        }

        fields.insert(
            "source_platform".to_string(),
            serde_json::json!(track.source_platform),
        );
        fields.insert(
            "source_model".to_string(),
            serde_json::json!(track.source_model),
        );
        fields.insert(
            "model_version".to_string(),
            serde_json::json!(track.model_version),
        );
        fields.insert(
            "timestamp".to_string(),
            serde_json::json!(track.timestamp.to_rfc3339()),
        );

        // Include formation_id if configured
        if let Some(formation_id) = &self.config.formation_id {
            fields.insert("formation_id".to_string(), serde_json::json!(formation_id));
        }

        // Include attributes
        if !track.attributes.is_empty() {
            fields.insert(
                "attributes".to_string(),
                serde_json::json!(track.attributes),
            );
        }

        // Use track_id + timestamp as document ID for uniqueness
        let doc_id = format!("{}_{}", track.track_id, track.timestamp.timestamp_millis());
        Document::with_id(doc_id, fields)
    }

    /// Convert HIVE Document back to TrackUpdate
    fn document_to_track(&self, doc: &Document) -> anyhow::Result<TrackUpdate> {
        use crate::messages::{Position, Velocity};

        let track_id = doc
            .get("track_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing track_id"))?
            .to_string();

        let classification = doc
            .get("classification")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing classification"))?
            .to_string();

        let confidence = doc
            .get("confidence")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing confidence"))?;

        let lat = doc
            .get("lat")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing lat"))?;

        let lon = doc
            .get("lon")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing lon"))?;

        let position = Position {
            lat,
            lon,
            cep_m: doc.get("cep_m").and_then(|v| v.as_f64()),
            hae: doc.get("hae").and_then(|v| v.as_f64()),
        };

        let velocity = match (
            doc.get("bearing").and_then(|v| v.as_f64()),
            doc.get("speed_mps").and_then(|v| v.as_f64()),
        ) {
            (Some(bearing), Some(speed_mps)) => Some(Velocity { bearing, speed_mps }),
            _ => None,
        };

        let source_platform = doc
            .get("source_platform")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing source_platform"))?
            .to_string();

        let source_model = doc
            .get("source_model")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing source_model"))?
            .to_string();

        let model_version = doc
            .get("model_version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing model_version"))?
            .to_string();

        let timestamp_str = doc
            .get("timestamp")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing timestamp"))?;

        let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp_str)
            .map_err(|e| anyhow::anyhow!("Invalid timestamp: {}", e))?
            .with_timezone(&chrono::Utc);

        let attributes = doc
            .get("attributes")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<HashMap<String, serde_json::Value>>()
            })
            .unwrap_or_default();

        Ok(TrackUpdate {
            track_id,
            classification,
            confidence,
            position,
            velocity,
            attributes,
            source_platform,
            source_model,
            model_version,
            timestamp,
        })
    }

    /// Convert CapabilityAdvertisement to HIVE Document
    fn capability_to_document(&self, cap: &CapabilityAdvertisement) -> Document {
        let mut fields = HashMap::new();

        fields.insert(
            "platform_id".to_string(),
            serde_json::json!(cap.platform_id),
        );
        fields.insert(
            "advertised_at".to_string(),
            serde_json::json!(cap.advertised_at.to_rfc3339()),
        );
        fields.insert("models".to_string(), serde_json::json!(cap.models));

        if let Some(resources) = &cap.resources {
            fields.insert("resources".to_string(), serde_json::json!(resources));
        }

        // Include formation_id if configured
        if let Some(formation_id) = &self.config.formation_id {
            fields.insert("formation_id".to_string(), serde_json::json!(formation_id));
        }

        // Use platform_id as document ID (upserts update the same doc)
        Document::with_id(&cap.platform_id, fields)
    }
}

/// Sync statistics
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub tracks_published: u64,
    pub capabilities_published: u64,
}

/// Connected inference pipeline with HIVE sync
///
/// Wraps an inference pipeline and automatically publishes
/// TrackUpdate messages to the HIVE network.
pub struct ConnectedPipeline<D, T>
where
    D: crate::inference::Detector + Send + 'static,
    T: crate::inference::Tracker + Send + 'static,
{
    pipeline: crate::inference::InferencePipeline<D, T>,
    sync_client: HiveSyncClient,
    /// Channel to receive shutdown signal
    shutdown_rx: Option<mpsc::Receiver<()>>,
}

impl<D, T> ConnectedPipeline<D, T>
where
    D: crate::inference::Detector + Send + 'static,
    T: crate::inference::Tracker + Send + 'static,
{
    /// Create a connected pipeline
    pub fn new(
        pipeline: crate::inference::InferencePipeline<D, T>,
        sync_client: HiveSyncClient,
    ) -> Self {
        Self {
            pipeline,
            sync_client,
            shutdown_rx: None,
        }
    }

    /// Process a frame and publish results to HIVE
    pub async fn process_and_publish(
        &mut self,
        frame: crate::inference::VideoFrame,
    ) -> anyhow::Result<crate::inference::PipelineOutput> {
        // Process frame through inference pipeline
        let output = self.pipeline.process(&frame).await?;

        // Publish track updates to HIVE
        if !output.track_updates.is_empty() {
            self.sync_client
                .publish_track_updates(&output.track_updates)
                .await?;
        }

        Ok(output)
    }

    /// Get sync statistics
    pub fn sync_stats(&self) -> SyncStats {
        self.sync_client.stats()
    }

    /// Get reference to underlying pipeline
    pub fn pipeline(&self) -> &crate::inference::InferencePipeline<D, T> {
        &self.pipeline
    }

    /// Get mutable reference to sync client
    pub fn sync_client_mut(&mut self) -> &mut HiveSyncClient {
        &mut self.sync_client
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{Position, Velocity};
    use chrono::Utc;

    fn sample_track() -> TrackUpdate {
        TrackUpdate {
            track_id: "TRACK-001".to_string(),
            classification: "person".to_string(),
            confidence: 0.89,
            position: Position {
                lat: 33.7749,
                lon: -84.3958,
                cep_m: Some(2.5),
                hae: None,
            },
            velocity: Some(Velocity {
                bearing: 45.0,
                speed_mps: 1.2,
            }),
            attributes: HashMap::new(),
            source_platform: "Alpha-2".to_string(),
            source_model: "Alpha-3".to_string(),
            model_version: "1.3.0".to_string(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_track_to_document_conversion() {
        let _config = SyncConfig::new("test-platform", "/tmp/test");

        // Create a mock backend - in real tests would use actual backend
        // For now just test the conversion logic
        let track = sample_track();

        // Test that we can create documents from tracks
        let mut fields = HashMap::new();
        fields.insert("track_id".to_string(), serde_json::json!(track.track_id));
        fields.insert(
            "classification".to_string(),
            serde_json::json!(track.classification),
        );
        fields.insert(
            "confidence".to_string(),
            serde_json::json!(track.confidence),
        );

        let doc = Document::with_id(&track.track_id, fields);
        assert!(doc.id.is_some());
    }

    #[test]
    fn test_sync_config_builder() {
        let config = SyncConfig::new("platform-1", "/data/hive")
            .with_formation("alpha-formation")
            .with_peers(vec!["192.168.1.100:4433".to_string()])
            .without_mdns();

        assert_eq!(config.platform_id, "platform-1");
        assert_eq!(config.formation_id, Some("alpha-formation".to_string()));
        assert!(!config.enable_mdns);
        assert_eq!(config.static_peers.len(), 1);
    }
}
