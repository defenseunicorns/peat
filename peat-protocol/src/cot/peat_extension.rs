//! Peat custom detail extension schema for CoT
//!
//! Implements the `<_peat_>` XML extension defined in ADR-028 for preserving
//! Peat-specific semantics in CoT messages.

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;

use super::event::CotError;
use super::types::OperationalStatus;

/// Peat version for the extension schema
pub const PEAT_EXTENSION_VERSION: &str = "1.0";

/// Peat custom detail extension
///
/// Contains Peat-specific metadata that doesn't map to standard CoT fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PeatExtension {
    /// Source attribution
    pub source: Option<PeatSource>,
    /// Confidence information
    pub confidence: Option<PeatConfidence>,
    /// Hierarchy membership
    pub hierarchy: Option<PeatHierarchy>,
    /// Custom attributes
    pub attributes: HashMap<String, PeatAttribute>,
    /// Operational status
    pub status: Option<PeatStatus>,
    /// Capability information (for capability advertisements)
    pub capabilities: Vec<PeatCapability>,
    /// Handoff information (for handoff messages)
    pub handoff: Option<PeatHandoff>,
    /// Classification information
    pub classification: Option<PeatClassification>,
}

impl PeatExtension {
    /// Create a new empty Peat extension
    pub fn new() -> Self {
        Self::default()
    }

    /// Set source attribution
    pub fn with_source(mut self, source: PeatSource) -> Self {
        self.source = Some(source);
        self
    }

    /// Set confidence
    pub fn with_confidence(mut self, value: f64, threshold: Option<f64>) -> Self {
        self.confidence = Some(PeatConfidence { value, threshold });
        self
    }

    /// Set hierarchy membership
    pub fn with_hierarchy(mut self, hierarchy: PeatHierarchy) -> Self {
        self.hierarchy = Some(hierarchy);
        self
    }

    /// Add an attribute
    pub fn with_attribute(mut self, key: &str, value: &str, attr_type: &str) -> Self {
        self.attributes.insert(
            key.to_string(),
            PeatAttribute {
                value: value.to_string(),
                attr_type: attr_type.to_string(),
            },
        );
        self
    }

    /// Set status
    pub fn with_status(mut self, status: PeatStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Add a capability
    pub fn with_capability(mut self, capability: PeatCapability) -> Self {
        self.capabilities.push(capability);
        self
    }

    /// Set handoff information
    pub fn with_handoff(mut self, handoff: PeatHandoff) -> Self {
        self.handoff = Some(handoff);
        self
    }

    /// Set classification
    pub fn with_classification(mut self, level: &str, caveat: Option<&str>) -> Self {
        self.classification = Some(PeatClassification {
            level: level.to_string(),
            caveat: caveat.map(|s| s.to_string()),
        });
        self
    }

    /// Write the extension as XML
    pub fn write_xml(&self, writer: &mut Writer<Cursor<Vec<u8>>>) -> Result<(), CotError> {
        let mut peat_elem = BytesStart::new("_peat_");
        peat_elem.push_attribute(("version", PEAT_EXTENSION_VERSION));

        writer
            .write_event(Event::Start(peat_elem))
            .map_err(|e| CotError::XmlWrite(e.to_string()))?;

        // Source
        if let Some(ref source) = self.source {
            let mut src_elem = BytesStart::new("source");
            src_elem.push_attribute(("platform", source.platform.as_str()));
            src_elem.push_attribute(("model", source.model.as_str()));
            src_elem.push_attribute(("model_version", source.model_version.as_str()));

            writer
                .write_event(Event::Empty(src_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Confidence
        if let Some(ref conf) = self.confidence {
            let mut conf_elem = BytesStart::new("confidence");
            conf_elem.push_attribute(("value", conf.value.to_string().as_str()));
            if let Some(threshold) = conf.threshold {
                conf_elem.push_attribute(("threshold", threshold.to_string().as_str()));
            }

            writer
                .write_event(Event::Empty(conf_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Hierarchy
        if let Some(ref hier) = self.hierarchy {
            writer
                .write_event(Event::Start(BytesStart::new("hierarchy")))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;

            if let Some(ref cell) = hier.cell {
                let mut cell_elem = BytesStart::new("cell");
                cell_elem.push_attribute(("id", cell.id.as_str()));
                if let Some(ref role) = cell.role {
                    cell_elem.push_attribute(("role", role.as_str()));
                }
                writer
                    .write_event(Event::Empty(cell_elem))
                    .map_err(|e| CotError::XmlWrite(e.to_string()))?;
            }

            if let Some(ref formation) = hier.formation {
                let mut form_elem = BytesStart::new("formation");
                form_elem.push_attribute(("id", formation.as_str()));
                writer
                    .write_event(Event::Empty(form_elem))
                    .map_err(|e| CotError::XmlWrite(e.to_string()))?;
            }

            if let Some(ref zone) = hier.zone {
                let mut zone_elem = BytesStart::new("zone");
                zone_elem.push_attribute(("id", zone.as_str()));
                writer
                    .write_event(Event::Empty(zone_elem))
                    .map_err(|e| CotError::XmlWrite(e.to_string()))?;
            }

            writer
                .write_event(Event::End(BytesEnd::new("hierarchy")))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Attributes
        if !self.attributes.is_empty() {
            writer
                .write_event(Event::Start(BytesStart::new("attributes")))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;

            for (key, attr) in &self.attributes {
                let mut attr_elem = BytesStart::new("attr");
                attr_elem.push_attribute(("key", key.as_str()));
                attr_elem.push_attribute(("type", attr.attr_type.as_str()));

                writer
                    .write_event(Event::Start(attr_elem))
                    .map_err(|e| CotError::XmlWrite(e.to_string()))?;
                writer
                    .write_event(Event::Text(BytesText::new(&attr.value)))
                    .map_err(|e| CotError::XmlWrite(e.to_string()))?;
                writer
                    .write_event(Event::End(BytesEnd::new("attr")))
                    .map_err(|e| CotError::XmlWrite(e.to_string()))?;
            }

            writer
                .write_event(Event::End(BytesEnd::new("attributes")))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Status
        if let Some(ref status) = self.status {
            let mut status_elem = BytesStart::new("status");
            status_elem.push_attribute(("operational", status.operational.as_str()));
            status_elem.push_attribute(("readiness", status.readiness.to_string().as_str()));

            writer
                .write_event(Event::Empty(status_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Capabilities
        for cap in &self.capabilities {
            let mut cap_elem = BytesStart::new("capability");
            cap_elem.push_attribute(("type", cap.capability_type.as_str()));
            cap_elem.push_attribute(("model_version", cap.model_version.as_str()));
            cap_elem.push_attribute(("precision", cap.precision.to_string().as_str()));
            cap_elem.push_attribute(("status", cap.status.as_str()));

            writer
                .write_event(Event::Empty(cap_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Handoff
        if let Some(ref handoff) = self.handoff {
            let mut handoff_elem = BytesStart::new("handoff");
            handoff_elem.push_attribute(("source_cell", handoff.source_cell.as_str()));
            handoff_elem.push_attribute(("target_cell", handoff.target_cell.as_str()));
            handoff_elem.push_attribute(("state", handoff.state.as_str()));
            handoff_elem.push_attribute(("reason", handoff.reason.as_str()));

            writer
                .write_event(Event::Empty(handoff_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Classification
        if let Some(ref class) = self.classification {
            let mut class_elem = BytesStart::new("classification");
            class_elem.push_attribute(("level", class.level.as_str()));
            if let Some(ref caveat) = class.caveat {
                class_elem.push_attribute(("caveat", caveat.as_str()));
            }

            writer
                .write_event(Event::Empty(class_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        writer
            .write_event(Event::End(BytesEnd::new("_peat_")))
            .map_err(|e| CotError::XmlWrite(e.to_string()))?;

        Ok(())
    }
}

/// Source attribution for track/capability origin
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeatSource {
    /// Platform that generated this data
    pub platform: String,
    /// Model/sensor name
    pub model: String,
    /// Model version
    pub model_version: String,
}

impl PeatSource {
    /// Create a new source attribution
    pub fn new(platform: &str, model: &str, model_version: &str) -> Self {
        Self {
            platform: platform.to_string(),
            model: model.to_string(),
            model_version: model_version.to_string(),
        }
    }
}

/// Confidence information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeatConfidence {
    /// Confidence value (0.0 - 1.0)
    pub value: f64,
    /// Optional threshold used
    pub threshold: Option<f64>,
}

/// Hierarchy membership information
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PeatHierarchy {
    /// Cell membership
    pub cell: Option<PeatCellMembership>,
    /// Formation membership
    pub formation: Option<String>,
    /// Zone membership
    pub zone: Option<String>,
}

impl PeatHierarchy {
    /// Create new hierarchy info
    pub fn new() -> Self {
        Self::default()
    }

    /// Set cell membership
    pub fn with_cell(mut self, id: &str, role: Option<&str>) -> Self {
        self.cell = Some(PeatCellMembership {
            id: id.to_string(),
            role: role.map(|s| s.to_string()),
        });
        self
    }

    /// Set formation
    pub fn with_formation(mut self, id: &str) -> Self {
        self.formation = Some(id.to_string());
        self
    }

    /// Set zone
    pub fn with_zone(mut self, id: &str) -> Self {
        self.zone = Some(id.to_string());
        self
    }
}

/// Cell membership with role
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeatCellMembership {
    /// Cell ID
    pub id: String,
    /// Role within cell (leader, member, tracker, etc.)
    pub role: Option<String>,
}

/// Custom attribute
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeatAttribute {
    /// Attribute value
    pub value: String,
    /// Attribute type (string, boolean, number, etc.)
    pub attr_type: String,
}

/// Operational status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeatStatus {
    /// Operational state
    pub operational: OperationalStatus,
    /// Readiness level (0.0 - 1.0)
    pub readiness: f64,
}

impl PeatStatus {
    /// Create a new status
    pub fn new(operational: OperationalStatus, readiness: f64) -> Self {
        Self {
            operational,
            readiness: readiness.clamp(0.0, 1.0),
        }
    }
}

/// Capability information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeatCapability {
    /// Capability type
    pub capability_type: String,
    /// Model/version
    pub model_version: String,
    /// Precision (0.0 - 1.0)
    pub precision: f64,
    /// Current status
    pub status: OperationalStatus,
}

impl PeatCapability {
    /// Create a new capability
    pub fn new(
        capability_type: &str,
        model_version: &str,
        precision: f64,
        status: OperationalStatus,
    ) -> Self {
        Self {
            capability_type: capability_type.to_string(),
            model_version: model_version.to_string(),
            precision: precision.clamp(0.0, 1.0),
            status,
        }
    }
}

/// Handoff information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeatHandoff {
    /// Source cell releasing track
    pub source_cell: String,
    /// Target cell receiving track
    pub target_cell: String,
    /// Current handoff state
    pub state: String,
    /// Reason for handoff
    pub reason: String,
}

impl PeatHandoff {
    /// Create new handoff info
    pub fn new(source_cell: &str, target_cell: &str, state: &str, reason: &str) -> Self {
        Self {
            source_cell: source_cell.to_string(),
            target_cell: target_cell.to_string(),
            state: state.to_string(),
            reason: reason.to_string(),
        }
    }
}

/// Classification information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeatClassification {
    /// Classification level
    pub level: String,
    /// Optional caveat
    pub caveat: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peat_extension_creation() {
        let ext = PeatExtension::new()
            .with_source(PeatSource::new("Alpha-2", "object_tracker", "1.3.0"))
            .with_confidence(0.89, Some(0.70))
            .with_hierarchy(
                PeatHierarchy::new()
                    .with_cell("Alpha-Team", Some("tracker"))
                    .with_formation("Formation-1"),
            )
            .with_attribute("jacket_color", "blue", "string")
            .with_status(PeatStatus::new(OperationalStatus::Active, 0.91));

        assert!(ext.source.is_some());
        assert_eq!(ext.source.as_ref().unwrap().platform, "Alpha-2");
        assert!(ext.confidence.is_some());
        assert_eq!(ext.confidence.as_ref().unwrap().value, 0.89);
    }

    #[test]
    fn test_peat_extension_to_xml() {
        let ext = PeatExtension::new()
            .with_source(PeatSource::new("Platform-1", "sensor", "1.0.0"))
            .with_confidence(0.85, None);

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        ext.write_xml(&mut writer).unwrap();

        let xml = String::from_utf8(writer.into_inner().into_inner()).unwrap();
        assert!(xml.contains("<_peat_"));
        assert!(xml.contains("version=\"1.0\""));
        assert!(xml.contains("platform=\"Platform-1\""));
        assert!(xml.contains("value=\"0.85\""));
    }

    #[test]
    fn test_peat_hierarchy() {
        let hier = PeatHierarchy::new()
            .with_cell("Alpha-Team", Some("leader"))
            .with_formation("Formation-1")
            .with_zone("Zone-A");

        assert_eq!(hier.cell.as_ref().unwrap().id, "Alpha-Team");
        assert_eq!(hier.cell.as_ref().unwrap().role, Some("leader".to_string()));
        assert_eq!(hier.formation, Some("Formation-1".to_string()));
        assert_eq!(hier.zone, Some("Zone-A".to_string()));
    }

    #[test]
    fn test_peat_capability() {
        let cap = PeatCapability::new("OBJECT_TRACKING", "1.3.0", 0.94, OperationalStatus::Active);

        assert_eq!(cap.capability_type, "OBJECT_TRACKING");
        assert_eq!(cap.precision, 0.94);
    }

    #[test]
    fn test_peat_status_readiness_clamped() {
        let status = PeatStatus::new(OperationalStatus::Active, 1.5);
        assert_eq!(status.readiness, 1.0);

        let status2 = PeatStatus::new(OperationalStatus::Degraded, -0.5);
        assert_eq!(status2.readiness, 0.0);
    }

    #[test]
    fn test_peat_extension_with_attributes() {
        let ext = PeatExtension::new()
            .with_attribute("color", "red", "string")
            .with_attribute("count", "5", "number")
            .with_attribute("active", "true", "boolean");

        assert_eq!(ext.attributes.len(), 3);
        assert_eq!(ext.attributes["color"].value, "red");
        assert_eq!(ext.attributes["color"].attr_type, "string");
    }

    #[test]
    fn test_peat_handoff() {
        let handoff =
            PeatHandoff::new("Alpha-Team", "Bravo-Team", "INITIATED", "boundary_crossing");

        assert_eq!(handoff.source_cell, "Alpha-Team");
        assert_eq!(handoff.target_cell, "Bravo-Team");
        assert_eq!(handoff.state, "INITIATED");
    }

    #[test]
    fn test_peat_classification() {
        let ext = PeatExtension::new().with_classification("UNCLASSIFIED", Some("FOUO"));

        assert!(ext.classification.is_some());
        let class = ext.classification.unwrap();
        assert_eq!(class.level, "UNCLASSIFIED");
        assert_eq!(class.caveat, Some("FOUO".to_string()));
    }
}
