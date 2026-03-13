//! MIL-STD-2525 symbol type mappings for CoT
//!
//! Maps Peat entity classifications to CoT type codes following MIL-STD-2525
//! military symbology standards.

use serde::{Deserialize, Serialize};

/// Entity affiliation (friend/hostile/unknown/neutral)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Affiliation {
    /// Friendly force
    Friendly,
    /// Hostile force
    Hostile,
    /// Unknown affiliation
    Unknown,
    /// Neutral entity
    Neutral,
    /// Assumed friendly
    AssumedFriendly,
    /// Suspect (assumed hostile)
    Suspect,
    /// Pending determination
    Pending,
}

impl Affiliation {
    /// Get the CoT affiliation character
    pub fn cot_char(&self) -> char {
        match self {
            Self::Friendly | Self::AssumedFriendly => 'f',
            Self::Hostile | Self::Suspect => 'h',
            Self::Neutral => 'n',
            Self::Unknown | Self::Pending => 'u',
        }
    }
}

/// Entity classification for type mapping
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityClassification {
    /// Tracked person/personnel
    Person,
    /// Ground vehicle
    Vehicle,
    /// Aircraft (fixed or rotary wing)
    Aircraft,
    /// UAV/Drone
    Uav,
    /// Maritime vessel
    Vessel,
    /// UGV/Ground robot
    Ugv,
    /// Sensor/Equipment
    Sensor,
    /// Military unit/team
    Unit,
    /// Unknown entity
    Unknown,
    /// Custom classification
    Custom(String),
}

impl EntityClassification {
    /// Parse from classification string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "person" | "personnel" | "human" | "dismount" => Self::Person,
            "vehicle" | "car" | "truck" | "tank" => Self::Vehicle,
            "aircraft" | "plane" | "helicopter" | "helo" => Self::Aircraft,
            "uav" | "drone" | "uas" => Self::Uav,
            "vessel" | "ship" | "boat" | "maritime" => Self::Vessel,
            "ugv" | "robot" | "ground_robot" => Self::Ugv,
            "sensor" | "equipment" => Self::Sensor,
            "unit" | "team" | "cell" | "squad" => Self::Unit,
            "unknown" | "" => Self::Unknown,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// CoT type code
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CotType(String);

impl CotType {
    /// Create a new CoT type
    pub fn new(type_code: &str) -> Self {
        Self(type_code.to_string())
    }

    /// Get the type code string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is an atom (position/location) type
    pub fn is_atom(&self) -> bool {
        self.0.starts_with("a-")
    }

    /// Check if this is a tasking type
    pub fn is_tasking(&self) -> bool {
        self.0.starts_with("t-")
    }

    /// Check if this is a drawing/shape type
    pub fn is_drawing(&self) -> bool {
        self.0.starts_with("u-d-")
    }
}

impl std::fmt::Display for CotType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Mapper for converting Peat classifications to CoT types
#[derive(Debug, Clone)]
pub struct CotTypeMapper {
    /// Custom type overrides
    custom_mappings: std::collections::HashMap<String, String>,
}

impl Default for CotTypeMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl CotTypeMapper {
    /// Create a new mapper with default MIL-STD-2525 mappings
    pub fn new() -> Self {
        Self {
            custom_mappings: std::collections::HashMap::new(),
        }
    }

    /// Add a custom mapping
    pub fn add_mapping(&mut self, classification: &str, cot_type: &str) {
        self.custom_mappings
            .insert(classification.to_lowercase(), cot_type.to_string());
    }

    /// Map a classification string to CoT type
    pub fn map(&self, classification: &str, affiliation: Affiliation) -> CotType {
        // Check custom mappings first
        if let Some(custom) = self.custom_mappings.get(&classification.to_lowercase()) {
            return CotType::new(custom);
        }

        let entity = EntityClassification::parse(classification);
        self.map_entity(&entity, affiliation)
    }

    /// Map an entity classification to CoT type
    pub fn map_entity(&self, entity: &EntityClassification, affiliation: Affiliation) -> CotType {
        let aff = affiliation.cot_char();

        let type_code = match entity {
            // Tracked person - Ground Equipment - Sensor (tracking a person)
            EntityClassification::Person => format!("a-{}-G-E-S", aff),

            // Ground vehicle
            EntityClassification::Vehicle => format!("a-{}-G-E-V", aff),

            // Aircraft (generic)
            EntityClassification::Aircraft => format!("a-{}-A", aff),

            // UAV - Air - Military - Fixed Wing - UAV
            EntityClassification::Uav => format!("a-{}-A-M-F-Q", aff),

            // Maritime vessel
            EntityClassification::Vessel => format!("a-{}-S", aff),

            // UGV - Ground Unit - Combat
            EntityClassification::Ugv => format!("a-{}-G-U-C", aff),

            // Sensor/Equipment
            EntityClassification::Sensor => format!("a-{}-G-E-S", aff),

            // Military unit/team
            EntityClassification::Unit => format!("a-{}-G-U-C", aff),

            // Unknown ground
            EntityClassification::Unknown => format!("a-{}-G", aff),

            // Custom - default to ground unknown
            EntityClassification::Custom(_) => format!("a-{}-G", aff),
        };

        CotType::new(&type_code)
    }

    /// Map a Peat platform type to CoT type
    pub fn map_platform(&self, platform_type: &str, affiliation: Affiliation) -> CotType {
        let aff = affiliation.cot_char();

        let type_code = match platform_type.to_lowercase().as_str() {
            "uav" | "drone" | "uas" => format!("a-{}-A-M-F-Q", aff), // UAV
            "ugv" | "robot" | "ground_robot" => format!("a-{}-G-U-C", aff), // UGV
            "soldier" | "operator" | "dismount" => format!("a-{}-G-U-C-I", aff), // Infantry
            "vehicle" | "humvee" | "mrap" => format!("a-{}-G-U-C-V", aff), // Combat Vehicle
            "command_vehicle" | "toc" => format!("a-{}-G-U-C-V-H", aff), // HQ Vehicle
            "sensor_platform" => format!("a-{}-G-E-S", aff),         // Sensor
            _ => format!("a-{}-G-U-C", aff),                         // Default to ground unit
        };

        CotType::new(&type_code)
    }

    /// Get the CoT type for a handoff event
    pub fn handoff_type() -> CotType {
        CotType::new("a-x-h-h")
    }

    /// Get the CoT type for a geofence/ROZ
    pub fn geofence_type() -> CotType {
        CotType::new("u-d-r")
    }

    /// Get the CoT type for a mission tasking
    pub fn mission_tasking_type() -> CotType {
        CotType::new("t-x-m-c")
    }

    /// Get the CoT type for a cell/team marker
    pub fn cell_marker_type(affiliation: Affiliation) -> CotType {
        let aff = affiliation.cot_char();
        CotType::new(&format!("a-{}-G-U-C", aff))
    }

    /// Get the CoT type for a formation marker
    pub fn formation_marker_type(affiliation: Affiliation) -> CotType {
        let aff = affiliation.cot_char();
        CotType::new(&format!("a-{}-G-U-C", aff))
    }
}

/// CoT relation types for link elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CotRelation {
    /// Parent (hierarchical ownership)
    Parent,
    /// Handoff (track transfer)
    Handoff,
    /// Sibling (same echelon)
    Sibling,
    /// Observing (sensor relationship)
    Observing,
}

impl CotRelation {
    /// Get the CoT relation code
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Parent => "p-p",
            Self::Handoff => "h-h",
            Self::Sibling => "s-s",
            Self::Observing => "o-o",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_affiliation_cot_char() {
        assert_eq!(Affiliation::Friendly.cot_char(), 'f');
        assert_eq!(Affiliation::Hostile.cot_char(), 'h');
        assert_eq!(Affiliation::Unknown.cot_char(), 'u');
        assert_eq!(Affiliation::Neutral.cot_char(), 'n');
        assert_eq!(Affiliation::AssumedFriendly.cot_char(), 'f');
        assert_eq!(Affiliation::Suspect.cot_char(), 'h');
    }

    #[test]
    fn test_entity_classification_parse() {
        assert_eq!(
            EntityClassification::parse("person"),
            EntityClassification::Person
        );
        assert_eq!(
            EntityClassification::parse("VEHICLE"),
            EntityClassification::Vehicle
        );
        assert_eq!(
            EntityClassification::parse("uav"),
            EntityClassification::Uav
        );
        assert_eq!(
            EntityClassification::parse("unknown"),
            EntityClassification::Unknown
        );
        assert!(matches!(
            EntityClassification::parse("custom_thing"),
            EntityClassification::Custom(_)
        ));
    }

    #[test]
    fn test_cot_type_mapper_person() {
        let mapper = CotTypeMapper::new();

        let friendly_person = mapper.map("person", Affiliation::Friendly);
        assert_eq!(friendly_person.as_str(), "a-f-G-E-S");

        let hostile_person = mapper.map("person", Affiliation::Hostile);
        assert_eq!(hostile_person.as_str(), "a-h-G-E-S");

        let unknown_person = mapper.map("person", Affiliation::Unknown);
        assert_eq!(unknown_person.as_str(), "a-u-G-E-S");
    }

    #[test]
    fn test_cot_type_mapper_vehicle() {
        let mapper = CotTypeMapper::new();

        let vehicle = mapper.map("vehicle", Affiliation::Friendly);
        assert_eq!(vehicle.as_str(), "a-f-G-E-V");
    }

    #[test]
    fn test_cot_type_mapper_uav() {
        let mapper = CotTypeMapper::new();

        let uav = mapper.map("uav", Affiliation::Friendly);
        assert_eq!(uav.as_str(), "a-f-A-M-F-Q");
    }

    #[test]
    fn test_cot_type_mapper_platform() {
        let mapper = CotTypeMapper::new();

        let ugv = mapper.map_platform("UGV", Affiliation::Friendly);
        assert_eq!(ugv.as_str(), "a-f-G-U-C");

        let operator = mapper.map_platform("operator", Affiliation::Friendly);
        assert_eq!(operator.as_str(), "a-f-G-U-C-I");
    }

    #[test]
    fn test_cot_type_mapper_custom() {
        let mut mapper = CotTypeMapper::new();
        mapper.add_mapping("special_target", "a-h-G-I-T");

        let custom = mapper.map("special_target", Affiliation::Hostile);
        assert_eq!(custom.as_str(), "a-h-G-I-T");
    }

    #[test]
    fn test_cot_type_is_atom() {
        let atom = CotType::new("a-f-G-E-S");
        assert!(atom.is_atom());

        let tasking = CotType::new("t-x-m-c");
        assert!(!tasking.is_atom());
    }

    #[test]
    fn test_cot_type_is_tasking() {
        let tasking = CotType::new("t-x-m-c");
        assert!(tasking.is_tasking());

        let atom = CotType::new("a-f-G-E-S");
        assert!(!atom.is_tasking());
    }

    #[test]
    fn test_cot_relation_strings() {
        assert_eq!(CotRelation::Parent.as_str(), "p-p");
        assert_eq!(CotRelation::Handoff.as_str(), "h-h");
        assert_eq!(CotRelation::Sibling.as_str(), "s-s");
        assert_eq!(CotRelation::Observing.as_str(), "o-o");
    }

    #[test]
    fn test_special_cot_types() {
        assert_eq!(CotTypeMapper::handoff_type().as_str(), "a-x-h-h");
        assert_eq!(CotTypeMapper::geofence_type().as_str(), "u-d-r");
        assert_eq!(CotTypeMapper::mission_tasking_type().as_str(), "t-x-m-c");
    }

    #[test]
    fn test_cell_and_formation_markers() {
        let cell = CotTypeMapper::cell_marker_type(Affiliation::Friendly);
        assert_eq!(cell.as_str(), "a-f-G-U-C");

        let formation = CotTypeMapper::formation_marker_type(Affiliation::Friendly);
        assert_eq!(formation.as_str(), "a-f-G-U-C");
    }
}
