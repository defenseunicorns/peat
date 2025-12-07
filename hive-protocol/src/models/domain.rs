//! Domain layer data structures
//!
//! This module defines the operational domain (altitude layer) for nodes and capabilities.
//! Domains affect sensor detection, engagement, and composition rules.

use serde::{Deserialize, Serialize};

/// Operational domain (altitude layer) for a node or capability
///
/// Domains define where a platform operates and what it can detect/engage.
/// Cross-domain operations require specific capability combinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(i32)]
pub enum Domain {
    /// Unspecified domain (defaults to Surface)
    #[default]
    Unspecified = 0,
    /// Subsurface operations (underwater, underground)
    /// - Submarines, UUVs, tunneling systems
    /// - Detectable only by sonar/acoustic sensors
    /// - Cannot directly engage air targets
    Subsurface = 1,
    /// Surface operations (ground, water surface)
    /// - Ground vehicles, ships, operators
    /// - Terrain affects movement and LOS
    /// - Can engage both subsurface (with ASW) and air (with ADA)
    Surface = 2,
    /// Air operations (airborne platforms)
    /// - Aircraft, drones, missiles
    /// - Full visibility, high mobility
    /// - Cannot detect subsurface without specialized sensors
    Air = 3,
}

impl Domain {
    /// Get the domain name as a string
    pub fn name(&self) -> &'static str {
        match self {
            Domain::Unspecified => "Unspecified",
            Domain::Subsurface => "Subsurface",
            Domain::Surface => "Surface",
            Domain::Air => "Air",
        }
    }

    /// Get the short code for display
    pub fn code(&self) -> &'static str {
        match self {
            Domain::Unspecified => "UNK",
            Domain::Subsurface => "SUB",
            Domain::Surface => "SFC",
            Domain::Air => "AIR",
        }
    }

    /// Check if this domain can natively detect the target domain
    ///
    /// Returns true if sensors in this domain can typically detect targets
    /// in the target domain without special equipment.
    pub fn can_detect(&self, target: Domain) -> bool {
        match (self, target) {
            // Same domain - always detectable
            (Domain::Subsurface, Domain::Subsurface) => true,
            (Domain::Surface, Domain::Surface) => true,
            (Domain::Air, Domain::Air) => true,

            // Air can see surface (looking down)
            (Domain::Air, Domain::Surface) => true,

            // Surface can see air (looking up) with appropriate sensors
            (Domain::Surface, Domain::Air) => true,

            // Subsurface can detect surface (periscope, passive sonar)
            (Domain::Subsurface, Domain::Surface) => true,

            // Surface can detect subsurface (with sonar/ASW)
            (Domain::Surface, Domain::Subsurface) => true,

            // Air cannot directly detect subsurface
            (Domain::Air, Domain::Subsurface) => false,

            // Subsurface cannot directly detect air
            (Domain::Subsurface, Domain::Air) => false,

            // Unspecified defaults to surface behavior
            (Domain::Unspecified, target) => Domain::Surface.can_detect(target),
            (source, Domain::Unspecified) => source.can_detect(Domain::Surface),
        }
    }

    /// Check if this domain can engage the target domain
    ///
    /// Returns true if weapons from this domain can reach targets in the target domain.
    pub fn can_engage(&self, target: Domain) -> bool {
        match (self, target) {
            // Same domain - always engageable
            (Domain::Subsurface, Domain::Subsurface) => true,
            (Domain::Surface, Domain::Surface) => true,
            (Domain::Air, Domain::Air) => true,

            // Air can strike surface
            (Domain::Air, Domain::Surface) => true,

            // Surface can engage air (ADA)
            (Domain::Surface, Domain::Air) => true,

            // Surface can engage subsurface (ASW)
            (Domain::Surface, Domain::Subsurface) => true,

            // Subsurface can engage surface (torpedoes, missiles)
            (Domain::Subsurface, Domain::Surface) => true,

            // Air can engage subsurface (ASW aircraft, sonobuoys + torpedoes)
            (Domain::Air, Domain::Subsurface) => true,

            // Subsurface typically cannot engage air directly
            (Domain::Subsurface, Domain::Air) => false,

            // Unspecified defaults to surface behavior
            (Domain::Unspecified, target) => Domain::Surface.can_engage(target),
            (source, Domain::Unspecified) => source.can_engage(Domain::Surface),
        }
    }

    /// Get all valid domains (excluding Unspecified)
    pub fn all() -> &'static [Domain] {
        &[Domain::Subsurface, Domain::Surface, Domain::Air]
    }

    /// Parse domain from string (case-insensitive)
    pub fn parse(s: &str) -> Option<Domain> {
        match s.to_lowercase().as_str() {
            "subsurface" | "sub" | "underwater" | "underground" => Some(Domain::Subsurface),
            "surface" | "sfc" | "ground" | "sea" => Some(Domain::Surface),
            "air" | "airborne" | "aerial" | "sky" => Some(Domain::Air),
            _ => None,
        }
    }
}

impl TryFrom<i32> for Domain {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Domain::Unspecified),
            1 => Ok(Domain::Subsurface),
            2 => Ok(Domain::Surface),
            3 => Ok(Domain::Air),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A set of domains that a capability or node can operate in
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DomainSet {
    subsurface: bool,
    surface: bool,
    air: bool,
}

impl DomainSet {
    /// Create an empty domain set
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a domain set with a single domain
    pub fn single(domain: Domain) -> Self {
        let mut set = Self::empty();
        set.add(domain);
        set
    }

    /// Create a domain set from multiple domains
    pub fn from_domains(domains: &[Domain]) -> Self {
        let mut set = Self::empty();
        for domain in domains {
            set.add(*domain);
        }
        set
    }

    /// Create a domain set covering all domains
    pub fn all() -> Self {
        Self {
            subsurface: true,
            surface: true,
            air: true,
        }
    }

    /// Add a domain to the set
    pub fn add(&mut self, domain: Domain) {
        match domain {
            Domain::Subsurface => self.subsurface = true,
            Domain::Surface => self.surface = true,
            Domain::Air => self.air = true,
            Domain::Unspecified => {} // No-op
        }
    }

    /// Remove a domain from the set
    pub fn remove(&mut self, domain: Domain) {
        match domain {
            Domain::Subsurface => self.subsurface = false,
            Domain::Surface => self.surface = false,
            Domain::Air => self.air = false,
            Domain::Unspecified => {} // No-op
        }
    }

    /// Check if the set contains a domain
    pub fn contains(&self, domain: Domain) -> bool {
        match domain {
            Domain::Subsurface => self.subsurface,
            Domain::Surface => self.surface,
            Domain::Air => self.air,
            Domain::Unspecified => true, // Unspecified matches any
        }
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        !self.subsurface && !self.surface && !self.air
    }

    /// Count the number of domains in the set
    pub fn count(&self) -> usize {
        (self.subsurface as usize) + (self.surface as usize) + (self.air as usize)
    }

    /// Check if this set covers multiple domains
    pub fn is_multi_domain(&self) -> bool {
        self.count() > 1
    }

    /// Get the intersection of two domain sets
    pub fn intersection(&self, other: &DomainSet) -> DomainSet {
        DomainSet {
            subsurface: self.subsurface && other.subsurface,
            surface: self.surface && other.surface,
            air: self.air && other.air,
        }
    }

    /// Get the union of two domain sets
    pub fn union(&self, other: &DomainSet) -> DomainSet {
        DomainSet {
            subsurface: self.subsurface || other.subsurface,
            surface: self.surface || other.surface,
            air: self.air || other.air,
        }
    }

    /// Iterate over the domains in the set
    pub fn iter(&self) -> impl Iterator<Item = Domain> + '_ {
        Domain::all().iter().copied().filter(|d| self.contains(*d))
    }

    /// Convert to a vector of domains
    pub fn to_vec(&self) -> Vec<Domain> {
        self.iter().collect()
    }
}

impl std::fmt::Display for DomainSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let domains: Vec<&str> = self.iter().map(|d| d.code()).collect();
        if domains.is_empty() {
            write!(f, "NONE")
        } else {
            write!(f, "{}", domains.join("+"))
        }
    }
}

/// Sensor type with associated domain constraints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensorType {
    /// Electro-optical (visible light cameras)
    /// Domains: Surface, Air (cannot penetrate water)
    ElectroOptical,
    /// Infrared (thermal imaging)
    /// Domains: Surface, Air (cannot penetrate water)
    Infrared,
    /// Radar (radio detection and ranging)
    /// Domains: Surface, Air (limited water penetration)
    Radar,
    /// Sonar (sound navigation and ranging)
    /// Domains: Subsurface, Surface (underwater only)
    Sonar,
    /// Acoustic (passive sound detection)
    /// Domains: All (sound propagates everywhere)
    Acoustic,
    /// SIGINT (signals intelligence)
    /// Domains: All (radio waves propagate through air/space)
    Sigint,
    /// Magnetic Anomaly Detector
    /// Domains: Air, Surface (for detecting subsurface)
    Mad,
}

impl SensorType {
    /// Get the domains this sensor type can operate in
    pub fn operating_domains(&self) -> DomainSet {
        match self {
            SensorType::ElectroOptical => DomainSet::from_domains(&[Domain::Surface, Domain::Air]),
            SensorType::Infrared => DomainSet::from_domains(&[Domain::Surface, Domain::Air]),
            SensorType::Radar => DomainSet::from_domains(&[Domain::Surface, Domain::Air]),
            SensorType::Sonar => DomainSet::from_domains(&[Domain::Subsurface, Domain::Surface]),
            SensorType::Acoustic => DomainSet::all(),
            SensorType::Sigint => DomainSet::all(),
            SensorType::Mad => DomainSet::from_domains(&[Domain::Surface, Domain::Air]),
        }
    }

    /// Get the domains this sensor type can detect targets in
    pub fn detection_domains(&self) -> DomainSet {
        match self {
            SensorType::ElectroOptical => DomainSet::from_domains(&[Domain::Surface, Domain::Air]),
            SensorType::Infrared => DomainSet::from_domains(&[Domain::Surface, Domain::Air]),
            SensorType::Radar => DomainSet::from_domains(&[Domain::Surface, Domain::Air]),
            SensorType::Sonar => DomainSet::from_domains(&[Domain::Subsurface, Domain::Surface]),
            SensorType::Acoustic => DomainSet::all(),
            SensorType::Sigint => DomainSet::all(),
            // MAD specifically detects subsurface from air/surface
            SensorType::Mad => DomainSet::single(Domain::Subsurface),
        }
    }

    /// Get the sensor type name
    pub fn name(&self) -> &'static str {
        match self {
            SensorType::ElectroOptical => "Electro-Optical",
            SensorType::Infrared => "Infrared",
            SensorType::Radar => "Radar",
            SensorType::Sonar => "Sonar",
            SensorType::Acoustic => "Acoustic",
            SensorType::Sigint => "SIGINT",
            SensorType::Mad => "MAD",
        }
    }

    /// Get the short code for display
    pub fn code(&self) -> &'static str {
        match self {
            SensorType::ElectroOptical => "EO",
            SensorType::Infrared => "IR",
            SensorType::Radar => "RAD",
            SensorType::Sonar => "SON",
            SensorType::Acoustic => "ACO",
            SensorType::Sigint => "SIG",
            SensorType::Mad => "MAD",
        }
    }
}

/// Domain-aware detection check result
#[derive(Debug, Clone, PartialEq)]
pub struct DetectionCheck {
    /// Can the sensor detect the target?
    pub can_detect: bool,
    /// Reason for the result
    pub reason: String,
    /// Detection modifier based on cross-domain factors
    pub modifier: i32,
}

impl DetectionCheck {
    /// Check if a sensor in one domain can detect a target in another domain
    pub fn check(
        sensor_domain: Domain,
        sensor_type: SensorType,
        target_domain: Domain,
    ) -> DetectionCheck {
        let operating = sensor_type.operating_domains();
        let detecting = sensor_type.detection_domains();

        // Sensor must be able to operate in its domain
        if !operating.contains(sensor_domain) {
            return DetectionCheck {
                can_detect: false,
                reason: format!(
                    "{} cannot operate in {} domain",
                    sensor_type.name(),
                    sensor_domain.name()
                ),
                modifier: 0,
            };
        }

        // Sensor must be able to detect targets in the target domain
        if !detecting.contains(target_domain) {
            return DetectionCheck {
                can_detect: false,
                reason: format!(
                    "{} cannot detect targets in {} domain",
                    sensor_type.name(),
                    target_domain.name()
                ),
                modifier: 0,
            };
        }

        // Cross-domain detection has penalties
        let modifier = if sensor_domain == target_domain {
            0 // Same domain - no penalty
        } else {
            match (sensor_domain, target_domain) {
                // Air looking down at surface - slight advantage
                (Domain::Air, Domain::Surface) => 1,
                // Surface looking up at air - slight penalty
                (Domain::Surface, Domain::Air) => -1,
                // Cross-domain ASW is harder
                (Domain::Air, Domain::Subsurface) => -2,
                (Domain::Surface, Domain::Subsurface) => -1,
                // Subsurface detecting surface - moderate
                (Domain::Subsurface, Domain::Surface) => -1,
                _ => 0,
            }
        };

        DetectionCheck {
            can_detect: true,
            reason: format!(
                "{} in {} can detect {} targets",
                sensor_type.name(),
                sensor_domain.name(),
                target_domain.name()
            ),
            modifier,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_names() {
        assert_eq!(Domain::Subsurface.name(), "Subsurface");
        assert_eq!(Domain::Surface.name(), "Surface");
        assert_eq!(Domain::Air.name(), "Air");
        assert_eq!(Domain::Unspecified.name(), "Unspecified");
    }

    #[test]
    fn test_domain_codes() {
        assert_eq!(Domain::Subsurface.code(), "SUB");
        assert_eq!(Domain::Surface.code(), "SFC");
        assert_eq!(Domain::Air.code(), "AIR");
    }

    #[test]
    fn test_domain_same_domain_detection() {
        assert!(Domain::Subsurface.can_detect(Domain::Subsurface));
        assert!(Domain::Surface.can_detect(Domain::Surface));
        assert!(Domain::Air.can_detect(Domain::Air));
    }

    #[test]
    fn test_domain_cross_domain_detection() {
        // Air can see surface
        assert!(Domain::Air.can_detect(Domain::Surface));
        // Surface can see air
        assert!(Domain::Surface.can_detect(Domain::Air));
        // Air cannot see subsurface directly
        assert!(!Domain::Air.can_detect(Domain::Subsurface));
        // Subsurface cannot see air
        assert!(!Domain::Subsurface.can_detect(Domain::Air));
        // Surface can detect subsurface (sonar)
        assert!(Domain::Surface.can_detect(Domain::Subsurface));
        // Subsurface can detect surface (periscope)
        assert!(Domain::Subsurface.can_detect(Domain::Surface));
    }

    #[test]
    fn test_domain_engagement() {
        // Same domain engagement
        assert!(Domain::Air.can_engage(Domain::Air));
        assert!(Domain::Surface.can_engage(Domain::Surface));
        assert!(Domain::Subsurface.can_engage(Domain::Subsurface));

        // Air can strike surface
        assert!(Domain::Air.can_engage(Domain::Surface));
        // Air can engage subsurface (ASW)
        assert!(Domain::Air.can_engage(Domain::Subsurface));
        // Surface can engage air (ADA)
        assert!(Domain::Surface.can_engage(Domain::Air));
        // Surface can engage subsurface (ASW)
        assert!(Domain::Surface.can_engage(Domain::Subsurface));
        // Subsurface can engage surface
        assert!(Domain::Subsurface.can_engage(Domain::Surface));
        // Subsurface cannot engage air directly
        assert!(!Domain::Subsurface.can_engage(Domain::Air));
    }

    #[test]
    fn test_domain_from_i32() {
        assert_eq!(Domain::try_from(0), Ok(Domain::Unspecified));
        assert_eq!(Domain::try_from(1), Ok(Domain::Subsurface));
        assert_eq!(Domain::try_from(2), Ok(Domain::Surface));
        assert_eq!(Domain::try_from(3), Ok(Domain::Air));
        assert!(Domain::try_from(99).is_err());
    }

    #[test]
    fn test_domain_from_str() {
        assert_eq!(Domain::parse("subsurface"), Some(Domain::Subsurface));
        assert_eq!(Domain::parse("SUB"), Some(Domain::Subsurface));
        assert_eq!(Domain::parse("underwater"), Some(Domain::Subsurface));
        assert_eq!(Domain::parse("surface"), Some(Domain::Surface));
        assert_eq!(Domain::parse("ground"), Some(Domain::Surface));
        assert_eq!(Domain::parse("air"), Some(Domain::Air));
        assert_eq!(Domain::parse("AIRBORNE"), Some(Domain::Air));
        assert_eq!(Domain::parse("invalid"), None);
    }

    #[test]
    fn test_domain_default() {
        assert_eq!(Domain::default(), Domain::Unspecified);
    }

    #[test]
    fn test_domain_all() {
        let all = Domain::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&Domain::Subsurface));
        assert!(all.contains(&Domain::Surface));
        assert!(all.contains(&Domain::Air));
        assert!(!all.contains(&Domain::Unspecified));
    }

    // DomainSet tests

    #[test]
    fn test_domain_set_empty() {
        let set = DomainSet::empty();
        assert!(set.is_empty());
        assert_eq!(set.count(), 0);
        assert!(!set.is_multi_domain());
    }

    #[test]
    fn test_domain_set_single() {
        let set = DomainSet::single(Domain::Air);
        assert!(!set.is_empty());
        assert_eq!(set.count(), 1);
        assert!(set.contains(Domain::Air));
        assert!(!set.contains(Domain::Surface));
        assert!(!set.is_multi_domain());
    }

    #[test]
    fn test_domain_set_all() {
        let set = DomainSet::all();
        assert_eq!(set.count(), 3);
        assert!(set.is_multi_domain());
        assert!(set.contains(Domain::Subsurface));
        assert!(set.contains(Domain::Surface));
        assert!(set.contains(Domain::Air));
    }

    #[test]
    fn test_domain_set_from_domains() {
        let set = DomainSet::from_domains(&[Domain::Air, Domain::Surface]);
        assert_eq!(set.count(), 2);
        assert!(set.is_multi_domain());
        assert!(set.contains(Domain::Air));
        assert!(set.contains(Domain::Surface));
        assert!(!set.contains(Domain::Subsurface));
    }

    #[test]
    fn test_domain_set_add_remove() {
        let mut set = DomainSet::empty();

        set.add(Domain::Air);
        assert!(set.contains(Domain::Air));
        assert_eq!(set.count(), 1);

        set.add(Domain::Surface);
        assert_eq!(set.count(), 2);

        set.remove(Domain::Air);
        assert!(!set.contains(Domain::Air));
        assert_eq!(set.count(), 1);
    }

    #[test]
    fn test_domain_set_intersection() {
        let set1 = DomainSet::from_domains(&[Domain::Air, Domain::Surface]);
        let set2 = DomainSet::from_domains(&[Domain::Surface, Domain::Subsurface]);

        let intersection = set1.intersection(&set2);
        assert_eq!(intersection.count(), 1);
        assert!(intersection.contains(Domain::Surface));
    }

    #[test]
    fn test_domain_set_union() {
        let set1 = DomainSet::from_domains(&[Domain::Air]);
        let set2 = DomainSet::from_domains(&[Domain::Surface]);

        let union = set1.union(&set2);
        assert_eq!(union.count(), 2);
        assert!(union.contains(Domain::Air));
        assert!(union.contains(Domain::Surface));
    }

    #[test]
    fn test_domain_set_iter() {
        let set = DomainSet::from_domains(&[Domain::Air, Domain::Subsurface]);
        let domains: Vec<Domain> = set.iter().collect();

        assert_eq!(domains.len(), 2);
        assert!(domains.contains(&Domain::Air));
        assert!(domains.contains(&Domain::Subsurface));
    }

    #[test]
    fn test_domain_set_display() {
        assert_eq!(DomainSet::empty().to_string(), "NONE");
        assert_eq!(DomainSet::single(Domain::Air).to_string(), "AIR");
        assert_eq!(
            DomainSet::from_domains(&[Domain::Air, Domain::Surface]).to_string(),
            "SFC+AIR"
        );
    }

    // SensorType tests

    #[test]
    fn test_sensor_type_eo_domains() {
        let eo = SensorType::ElectroOptical;
        let operating = eo.operating_domains();
        let detecting = eo.detection_domains();

        assert!(operating.contains(Domain::Surface));
        assert!(operating.contains(Domain::Air));
        assert!(!operating.contains(Domain::Subsurface));

        assert!(detecting.contains(Domain::Surface));
        assert!(detecting.contains(Domain::Air));
        assert!(!detecting.contains(Domain::Subsurface));
    }

    #[test]
    fn test_sensor_type_sonar_domains() {
        let sonar = SensorType::Sonar;
        let operating = sonar.operating_domains();
        let detecting = sonar.detection_domains();

        assert!(operating.contains(Domain::Subsurface));
        assert!(operating.contains(Domain::Surface));
        assert!(!operating.contains(Domain::Air));

        assert!(detecting.contains(Domain::Subsurface));
        assert!(detecting.contains(Domain::Surface));
    }

    #[test]
    fn test_sensor_type_mad_domains() {
        let mad = SensorType::Mad;
        let operating = mad.operating_domains();
        let detecting = mad.detection_domains();

        // MAD operates from air/surface
        assert!(operating.contains(Domain::Air));
        assert!(operating.contains(Domain::Surface));
        // MAD specifically detects subsurface
        assert!(detecting.contains(Domain::Subsurface));
        assert_eq!(detecting.count(), 1);
    }

    #[test]
    fn test_sensor_type_acoustic_all_domains() {
        let acoustic = SensorType::Acoustic;
        let operating = acoustic.operating_domains();

        assert_eq!(operating.count(), 3);
        assert!(operating.contains(Domain::Subsurface));
        assert!(operating.contains(Domain::Surface));
        assert!(operating.contains(Domain::Air));
    }

    // DetectionCheck tests

    #[test]
    fn test_detection_check_same_domain() {
        let check = DetectionCheck::check(Domain::Air, SensorType::Radar, Domain::Air);
        assert!(check.can_detect);
        assert_eq!(check.modifier, 0);
    }

    #[test]
    fn test_detection_check_air_to_surface() {
        let check = DetectionCheck::check(Domain::Air, SensorType::ElectroOptical, Domain::Surface);
        assert!(check.can_detect);
        assert_eq!(check.modifier, 1); // Advantage from altitude
    }

    #[test]
    fn test_detection_check_surface_to_air() {
        let check = DetectionCheck::check(Domain::Surface, SensorType::Radar, Domain::Air);
        assert!(check.can_detect);
        assert_eq!(check.modifier, -1); // Slight penalty looking up
    }

    #[test]
    fn test_detection_check_eo_cannot_detect_subsurface() {
        let check = DetectionCheck::check(
            Domain::Surface,
            SensorType::ElectroOptical,
            Domain::Subsurface,
        );
        assert!(!check.can_detect);
        assert!(check.reason.contains("cannot detect"));
    }

    #[test]
    fn test_detection_check_sonar_cannot_operate_in_air() {
        let check = DetectionCheck::check(Domain::Air, SensorType::Sonar, Domain::Subsurface);
        assert!(!check.can_detect);
        assert!(check.reason.contains("cannot operate"));
    }

    #[test]
    fn test_detection_check_mad_from_air_to_subsurface() {
        let check = DetectionCheck::check(Domain::Air, SensorType::Mad, Domain::Subsurface);
        assert!(check.can_detect);
        assert_eq!(check.modifier, -2); // Cross-domain ASW penalty
    }

    #[test]
    fn test_detection_check_sonar_subsurface_to_surface() {
        let check = DetectionCheck::check(Domain::Subsurface, SensorType::Sonar, Domain::Surface);
        assert!(check.can_detect);
        assert_eq!(check.modifier, -1); // Cross-domain penalty
    }
}
