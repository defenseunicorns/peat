//! HIVE → CoT message encoder
//!
//! Converts HIVE messages to CoT XML format for TAK integration.

use chrono::Duration;

use super::event::{CotError, CotEvent, CotLink, CotPoint};
use super::hive_extension::{
    HiveCapability, HiveExtension, HiveHandoff, HiveHierarchy, HiveSource, HiveStatus,
};
use super::type_mapper::{Affiliation, CotRelation, CotTypeMapper};
use super::types::{
    CapabilityAdvertisement, FormationCapabilitySummary, HandoffMessage, TrackUpdate,
};

/// Configuration for CoT encoding
#[derive(Debug, Clone)]
pub struct CotEncoderConfig {
    /// Default stale duration for track updates
    pub track_stale_secs: i64,
    /// Default stale duration for capability advertisements
    pub capability_stale_secs: i64,
    /// Default stale duration for handoff messages
    pub handoff_stale_secs: i64,
    /// Default affiliation for HIVE entities
    pub default_affiliation: Affiliation,
    /// Include HIVE extension in output
    pub include_hive_extension: bool,
}

impl Default for CotEncoderConfig {
    fn default() -> Self {
        Self {
            track_stale_secs: 30,
            capability_stale_secs: 60,
            handoff_stale_secs: 300,
            default_affiliation: Affiliation::Friendly,
            include_hive_extension: true,
        }
    }
}

/// Encoder for converting HIVE messages to CoT XML
#[derive(Debug, Clone)]
pub struct CotEncoder {
    /// Configuration
    config: CotEncoderConfig,
    /// Type mapper
    type_mapper: CotTypeMapper,
}

impl Default for CotEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl CotEncoder {
    /// Create a new encoder with default configuration
    pub fn new() -> Self {
        Self {
            config: CotEncoderConfig::default(),
            type_mapper: CotTypeMapper::new(),
        }
    }

    /// Create encoder with custom configuration
    pub fn with_config(config: CotEncoderConfig) -> Self {
        Self {
            config,
            type_mapper: CotTypeMapper::new(),
        }
    }

    /// Get mutable reference to type mapper for adding custom mappings
    pub fn type_mapper_mut(&mut self) -> &mut CotTypeMapper {
        &mut self.type_mapper
    }

    /// Build a CotEvent from a TrackUpdate
    pub fn track_update_to_event(&self, track: &TrackUpdate) -> Result<CotEvent, CotError> {
        let cot_type = self
            .type_mapper
            .map(&track.classification, self.config.default_affiliation);

        let mut builder = CotEvent::builder()
            .uid(&track.track_id)
            .cot_type(cot_type)
            .time(track.timestamp)
            .stale_duration(Duration::seconds(self.config.track_stale_secs))
            .point(CotPoint::with_full(
                track.position.lat,
                track.position.lon,
                track.position.hae.unwrap_or(0.0),
                track.position.cep_m.unwrap_or(9999999.0),
                9999999.0, // LE unknown
            ))
            .remarks(&self.format_track_remarks(track));

        // Add track velocity if present
        if let Some(ref vel) = track.velocity {
            builder = builder.track(vel.bearing, vel.speed_mps);
        }

        // Add HIVE extension
        if self.config.include_hive_extension {
            let mut ext = HiveExtension::new()
                .with_source(HiveSource::new(
                    &track.source_platform,
                    &track.source_model,
                    &track.model_version,
                ))
                .with_confidence(track.confidence, Some(0.70));

            // Add hierarchy if present
            if track.cell_id.is_some() || track.formation_id.is_some() {
                let mut hier = HiveHierarchy::new();
                if let Some(ref cell_id) = track.cell_id {
                    hier = hier.with_cell(cell_id, Some("tracker"));
                }
                if let Some(ref formation_id) = track.formation_id {
                    hier = hier.with_formation(formation_id);
                }
                ext = ext.with_hierarchy(hier);
            }

            // Add attributes
            for (key, value) in &track.attributes {
                let (val_str, type_str) = self.json_value_to_attr(value);
                ext = ext.with_attribute(key, &val_str, &type_str);
            }

            builder = builder.hive_extension(ext);
        }

        // Add link to source platform
        builder = builder.link(
            CotLink::new(&track.source_platform, "a-f-G-U-C", CotRelation::Observing)
                .with_remarks("sensor-platform"),
        );

        // Add hierarchy links
        if let Some(ref cell_id) = track.cell_id {
            builder = builder.link(
                CotLink::new(cell_id, "a-f-G-U-C", CotRelation::Parent).with_remarks("parent-cell"),
            );
        }

        builder.build()
    }

    /// Encode a TrackUpdate to CoT XML
    pub fn encode_track_update(&self, track: &TrackUpdate) -> Result<String, CotError> {
        self.track_update_to_event(track)?.to_xml()
    }

    /// Build a CotEvent from a CapabilityAdvertisement
    pub fn capability_to_event(&self, cap: &CapabilityAdvertisement) -> Result<CotEvent, CotError> {
        let cot_type = self
            .type_mapper
            .map_platform(&cap.platform_type, self.config.default_affiliation);

        let mut builder = CotEvent::builder()
            .uid(&cap.platform_id)
            .cot_type(cot_type)
            .time(cap.timestamp)
            .stale_duration(Duration::seconds(self.config.capability_stale_secs))
            .point(CotPoint::with_full(
                cap.position.lat,
                cap.position.lon,
                cap.position.hae.unwrap_or(0.0),
                cap.position.cep_m.unwrap_or(9999999.0),
                9999999.0,
            ))
            .callsign(&cap.platform_id)
            .remarks(&self.format_capability_remarks(cap));

        // Add group membership if cell assigned
        if let Some(ref cell_id) = cap.cell_id {
            builder = builder.group(cell_id, "Team Member");
        }

        // Add HIVE extension
        if self.config.include_hive_extension {
            let mut ext =
                HiveExtension::new().with_status(HiveStatus::new(cap.status, cap.readiness));

            // Add hierarchy
            if cap.cell_id.is_some() || cap.formation_id.is_some() {
                let mut hier = HiveHierarchy::new();
                if let Some(ref cell_id) = cap.cell_id {
                    hier = hier.with_cell(cell_id, None);
                }
                if let Some(ref formation_id) = cap.formation_id {
                    hier = hier.with_formation(formation_id);
                }
                ext = ext.with_hierarchy(hier);
            }

            // Add capabilities
            for cap_info in &cap.capabilities {
                ext = ext.with_capability(HiveCapability::new(
                    &cap_info.capability_type,
                    &cap_info.version,
                    cap_info.precision,
                    cap_info.status,
                ));
            }

            builder = builder.hive_extension(ext);
        }

        // Add hierarchy links
        if let Some(ref cell_id) = cap.cell_id {
            builder = builder.link(
                CotLink::new(cell_id, "a-f-G-U-C", CotRelation::Parent).with_remarks("parent-cell"),
            );
        }

        builder.build()
    }

    /// Encode a CapabilityAdvertisement to CoT XML
    pub fn encode_capability_advertisement(
        &self,
        cap: &CapabilityAdvertisement,
    ) -> Result<String, CotError> {
        self.capability_to_event(cap)?.to_xml()
    }

    /// Build a CotEvent from a HandoffMessage
    pub fn handoff_to_event(&self, handoff: &HandoffMessage) -> Result<CotEvent, CotError> {
        let cot_type = CotTypeMapper::handoff_type();

        let mut builder = CotEvent::builder()
            .uid(&format!("HANDOFF-{}", handoff.track_id))
            .cot_type(cot_type)
            .time(handoff.timestamp)
            .stale_duration(Duration::seconds(self.config.handoff_stale_secs))
            .point(CotPoint::with_full(
                handoff.position.lat,
                handoff.position.lon,
                handoff.position.hae.unwrap_or(0.0),
                handoff.position.cep_m.unwrap_or(9999999.0),
                9999999.0,
            ))
            .remarks(&format!(
                "Track {} handoff: {} → {} ({})",
                handoff.track_id, handoff.source_cell, handoff.target_cell, handoff.reason
            ));

        // Map priority to flow tags
        let flow_priority = match handoff.priority {
            1 => "flash",
            2 => "immediate",
            3 => "routine",
            4 => "deferred",
            _ => "bulk",
        };
        builder = builder.flow_priority(flow_priority);

        // Add HIVE extension
        if self.config.include_hive_extension {
            let ext = HiveExtension::new().with_handoff(HiveHandoff::new(
                &handoff.source_cell,
                &handoff.target_cell,
                handoff.state.as_str(),
                &handoff.reason,
            ));

            builder = builder.hive_extension(ext);
        }

        // Add links to cells
        builder = builder
            .link(
                CotLink::new(&handoff.source_cell, "a-f-G-U-C", CotRelation::Handoff)
                    .with_remarks("handoff-source"),
            )
            .link(
                CotLink::new(&handoff.target_cell, "a-f-G-U-C", CotRelation::Handoff)
                    .with_remarks("handoff-target"),
            )
            .link(
                CotLink::new(&handoff.track_id, "a-f-G-E-S", CotRelation::Observing)
                    .with_remarks("handoff-track"),
            );

        builder.build()
    }

    /// Encode a HandoffMessage to CoT XML
    pub fn encode_handoff(&self, handoff: &HandoffMessage) -> Result<String, CotError> {
        self.handoff_to_event(handoff)?.to_xml()
    }

    /// Build a CotEvent from a FormationCapabilitySummary
    pub fn formation_summary_to_event(
        &self,
        summary: &FormationCapabilitySummary,
    ) -> Result<CotEvent, CotError> {
        let cot_type = CotTypeMapper::formation_marker_type(self.config.default_affiliation);

        let mut builder = CotEvent::builder()
            .uid(&summary.formation_id)
            .cot_type(cot_type)
            .time(summary.timestamp)
            .stale_duration(Duration::seconds(self.config.capability_stale_secs))
            .point(CotPoint::with_full(
                summary.center_position.lat,
                summary.center_position.lon,
                summary.center_position.hae.unwrap_or(0.0),
                summary.center_position.cep_m.unwrap_or(9999999.0),
                9999999.0,
            ))
            .callsign(&summary.callsign)
            .remarks(&format!(
                "Formation {} - {} platforms, {} cells, {:.0}% ready",
                summary.callsign,
                summary.platform_count,
                summary.cell_count,
                summary.readiness * 100.0
            ));

        // Add HIVE extension with aggregated capabilities
        if self.config.include_hive_extension {
            let mut ext = HiveExtension::new()
                .with_status(HiveStatus::new(
                    super::types::OperationalStatus::Active,
                    summary.readiness,
                ))
                .with_hierarchy(HiveHierarchy::new().with_formation(&summary.formation_id));

            // Add aggregated capabilities
            for agg_cap in &summary.capabilities {
                ext = ext.with_capability(HiveCapability::new(
                    &agg_cap.capability_type,
                    &format!("{} units", agg_cap.count),
                    agg_cap.avg_precision,
                    if agg_cap.availability > 0.8 {
                        super::types::OperationalStatus::Active
                    } else if agg_cap.availability > 0.5 {
                        super::types::OperationalStatus::Degraded
                    } else {
                        super::types::OperationalStatus::Offline
                    },
                ));
            }

            builder = builder.hive_extension(ext);
        }

        builder.build()
    }

    /// Encode a FormationCapabilitySummary to CoT XML
    pub fn encode_formation_summary(
        &self,
        summary: &FormationCapabilitySummary,
    ) -> Result<String, CotError> {
        self.formation_summary_to_event(summary)?.to_xml()
    }

    fn format_track_remarks(&self, track: &TrackUpdate) -> String {
        let mut remarks = format!(
            "{}: {:.0}% confidence",
            track.classification,
            track.confidence * 100.0
        );

        // Add key attributes
        for (key, value) in &track.attributes {
            if let serde_json::Value::String(s) = value {
                remarks.push_str(&format!(", {}={}", key, s));
            } else if let serde_json::Value::Bool(b) = value {
                if *b {
                    remarks.push_str(&format!(", {}", key));
                }
            }
        }

        remarks
    }

    fn format_capability_remarks(&self, cap: &CapabilityAdvertisement) -> String {
        let cap_list: Vec<_> = cap
            .capabilities
            .iter()
            .map(|c| c.capability_type.as_str())
            .collect();

        format!(
            "{} ({}) - {} ({:.0}% ready)",
            cap.platform_type,
            cap_list.join(", "),
            cap.status.as_str(),
            cap.readiness * 100.0
        )
    }

    fn json_value_to_attr(&self, value: &serde_json::Value) -> (String, String) {
        match value {
            serde_json::Value::String(s) => (s.clone(), "string".to_string()),
            serde_json::Value::Bool(b) => (b.to_string(), "boolean".to_string()),
            serde_json::Value::Number(n) => (n.to_string(), "number".to_string()),
            _ => (value.to_string(), "json".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cot::types::{CapabilityInfo, OperationalStatus, Position, Velocity};

    #[test]
    fn test_encode_track_update() {
        let encoder = CotEncoder::new();

        let track = TrackUpdate::new(
            "TRACK-001".to_string(),
            "person".to_string(),
            0.89,
            Position::with_accuracy(33.7749, -84.3958, 2.5),
            "Alpha-2".to_string(),
            "object_tracker".to_string(),
            "1.3.0".to_string(),
        )
        .with_velocity(Velocity::new(45.0, 1.2))
        .with_attribute("jacket_color", serde_json::json!("blue"))
        .with_cell("Alpha-Team".to_string());

        let xml = encoder.encode_track_update(&track).unwrap();

        assert!(xml.contains("uid=\"TRACK-001\""));
        assert!(xml.contains("type=\"a-f-G-E-S\""));
        assert!(xml.contains("lat=\"33.7749\""));
        assert!(xml.contains("<_hive_"));
        assert!(xml.contains("platform=\"Alpha-2\""));
        assert!(xml.contains("jacket_color"));
    }

    #[test]
    fn test_encode_capability_advertisement() {
        let encoder = CotEncoder::new();

        let cap = CapabilityAdvertisement::new(
            "Alpha-3".to_string(),
            "UGV".to_string(),
            Position::new(33.7749, -84.3958),
            OperationalStatus::Active,
            0.91,
        )
        .with_capability(CapabilityInfo {
            capability_type: "OBJECT_TRACKING".to_string(),
            model_name: "object_tracker".to_string(),
            version: "1.3.0".to_string(),
            precision: 0.94,
            status: OperationalStatus::Active,
        })
        .with_cell("Alpha-Team".to_string());

        let xml = encoder.encode_capability_advertisement(&cap).unwrap();

        assert!(xml.contains("uid=\"Alpha-3\""));
        assert!(xml.contains("callsign=\"Alpha-3\""));
        assert!(xml.contains("__group"));
        assert!(xml.contains("<capability"));
    }

    #[test]
    fn test_encode_handoff() {
        let encoder = CotEncoder::new();

        let handoff = HandoffMessage::new(
            "TRACK-001".to_string(),
            Position::new(33.78, -84.40),
            "Alpha-Team".to_string(),
            "Bravo-Team".to_string(),
            "boundary_crossing".to_string(),
        )
        .with_priority(2);

        let xml = encoder.encode_handoff(&handoff).unwrap();

        assert!(xml.contains("uid=\"HANDOFF-TRACK-001\""));
        assert!(xml.contains("type=\"a-x-h-h\""));
        assert!(xml.contains("<handoff"));
        assert!(xml.contains("priority=\"immediate\""));
    }

    #[test]
    fn test_encoder_without_hive_extension() {
        let config = CotEncoderConfig {
            include_hive_extension: false,
            ..Default::default()
        };

        let encoder = CotEncoder::with_config(config);

        let track = TrackUpdate::new(
            "TRACK-001".to_string(),
            "person".to_string(),
            0.89,
            Position::new(0.0, 0.0),
            "platform".to_string(),
            "model".to_string(),
            "1.0".to_string(),
        );

        let xml = encoder.encode_track_update(&track).unwrap();

        assert!(!xml.contains("<_hive_"));
    }

    #[test]
    fn test_priority_to_flow_tags() {
        let encoder = CotEncoder::new();

        for (priority, expected_tag) in [
            (1u8, "flash"),
            (2, "immediate"),
            (3, "routine"),
            (4, "deferred"),
            (5, "bulk"),
        ] {
            let handoff = HandoffMessage::new(
                "TRACK".to_string(),
                Position::new(0.0, 0.0),
                "src".to_string(),
                "dst".to_string(),
                "test".to_string(),
            )
            .with_priority(priority);

            let xml = encoder.encode_handoff(&handoff).unwrap();
            assert!(
                xml.contains(&format!("priority=\"{}\"", expected_tag)),
                "Priority {} should map to {}",
                priority,
                expected_tag
            );
        }
    }

    #[test]
    fn test_custom_type_mapping() {
        let mut encoder = CotEncoder::new();
        encoder
            .type_mapper_mut()
            .add_mapping("special_target", "a-h-G-I-T");

        let track = TrackUpdate::new(
            "TRACK-001".to_string(),
            "special_target".to_string(),
            0.95,
            Position::new(0.0, 0.0),
            "platform".to_string(),
            "model".to_string(),
            "1.0".to_string(),
        );

        let xml = encoder.encode_track_update(&track).unwrap();
        assert!(xml.contains("type=\"a-h-G-I-T\""));
    }
}
