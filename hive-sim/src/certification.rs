//! Certification lifecycle management for HIVE simulation
//!
//! Models hazmat certification expiry and recertification flow for port-ops workers.
//! Workers hold hazmat certifications for specific classes (3, 8, 9) with explicit
//! expiry dates. As sim_time progresses:
//!
//! - Within 30 days of expiry: emit `CertificationExpiring` warning
//! - On expiry: capability downgrades (`certification_valid: false`)
//! - Recertification: 45-min sim_time logistical action restores capability
//! - Evidence-chain path: experienced workers get reduced confidence (0.7 vs 1.0)

use std::collections::HashMap;

use crate::metrics::{log_metrics, MetricsEvent};

/// Simulated time constants (in microseconds)
const MICROS_PER_SECOND: u64 = 1_000_000;
const MICROS_PER_MINUTE: u64 = 60 * MICROS_PER_SECOND;
const MICROS_PER_DAY: u64 = 24 * 60 * MICROS_PER_MINUTE;

/// Warning threshold: 30 days before expiry
const EXPIRY_WARNING_DAYS: u64 = 30;
const EXPIRY_WARNING_MICROS: u64 = EXPIRY_WARNING_DAYS * MICROS_PER_DAY;

/// Recertification duration: 45 minutes sim_time
const RECERTIFICATION_DURATION_MICROS: u64 = 45 * MICROS_PER_MINUTE;

/// Confidence for fully certified worker
const FULL_CERTIFICATION_CONFIDENCE: f32 = 1.0;

/// Confidence for expired-with-evidence-chain worker
const EVIDENCE_CHAIN_CONFIDENCE: f32 = 0.7;

/// Hazmat classes supported in port operations
pub const HAZMAT_CLASSES: &[u8] = &[3, 8, 9];

/// Represents a single hazmat certification for a worker
#[derive(Debug, Clone)]
pub struct HazmatCertification {
    /// Hazmat class (3, 8, or 9)
    pub hazmat_class: u8,
    /// When the cert was issued (sim_time microseconds)
    pub issued_at: u64,
    /// When the cert expires (sim_time microseconds)
    pub expires_at: u64,
    /// Whether the cert is currently valid
    pub certification_valid: bool,
    /// Number of hazmat handlings performed under this cert
    pub handling_count: u64,
    /// Number of incidents during handlings
    pub incident_count: u64,
    /// Whether the expiry warning has been emitted
    warning_emitted: bool,
    /// Whether the expiry event has been emitted
    expiry_emitted: bool,
}

/// Tracks an in-progress recertification action
#[derive(Debug, Clone)]
pub struct RecertificationAction {
    /// Hazmat class being recertified
    pub hazmat_class: u8,
    /// When recertification started (sim_time microseconds)
    pub started_at: u64,
    /// When recertification completes (sim_time microseconds)
    pub completes_at: u64,
}

/// Manages certification lifecycle for a single worker
#[derive(Debug)]
pub struct CertificationManager {
    worker_id: String,
    /// Active certifications by hazmat class
    certifications: HashMap<u8, HazmatCertification>,
    /// In-progress recertification actions
    pending_recertifications: Vec<RecertificationAction>,
}

/// Events produced by the certification lifecycle
#[derive(Debug, Clone, PartialEq)]
pub enum CertificationEvent {
    /// Certification approaching expiry (within 30 days)
    Expiring {
        hazmat_class: u8,
        days_remaining: u64,
    },
    /// Certification has expired, capability downgraded
    Expired {
        hazmat_class: u8,
        handling_count: u64,
        incident_count: u64,
    },
    /// Recertification started (45 min sim_time)
    RecertificationStarted {
        hazmat_class: u8,
    },
    /// Recertification completed, capability restored
    RecertificationCompleted {
        hazmat_class: u8,
        new_confidence: f32,
    },
}

impl HazmatCertification {
    /// Create a new certification with the given validity period
    pub fn new(hazmat_class: u8, issued_at: u64, validity_days: u64) -> Self {
        Self {
            hazmat_class,
            issued_at,
            expires_at: issued_at + validity_days * MICROS_PER_DAY,
            certification_valid: true,
            handling_count: 0,
            incident_count: 0,
            warning_emitted: false,
            expiry_emitted: false,
        }
    }

    /// Check if this cert has an evidence chain (experienced handler, clean record)
    pub fn has_evidence_chain(&self) -> bool {
        self.handling_count > 0 && self.incident_count == 0
    }

    /// Get the confidence level for this certification's capability
    pub fn capability_confidence(&self) -> f32 {
        if self.certification_valid {
            FULL_CERTIFICATION_CONFIDENCE
        } else if self.has_evidence_chain() {
            EVIDENCE_CHAIN_CONFIDENCE
        } else {
            0.0
        }
    }

    /// Days remaining until expiry (0 if expired)
    pub fn days_remaining(&self, now: u64) -> u64 {
        if now >= self.expires_at {
            0
        } else {
            (self.expires_at - now) / MICROS_PER_DAY
        }
    }
}

impl CertificationManager {
    /// Create a new manager for the given worker
    pub fn new(worker_id: String) -> Self {
        Self {
            worker_id,
            certifications: HashMap::new(),
            pending_recertifications: Vec::new(),
        }
    }

    /// Add a hazmat certification for this worker
    pub fn add_certification(&mut self, cert: HazmatCertification) {
        self.certifications.insert(cert.hazmat_class, cert);
    }

    /// Record a handling event for a hazmat class
    pub fn record_handling(&mut self, hazmat_class: u8, incident: bool) {
        if let Some(cert) = self.certifications.get_mut(&hazmat_class) {
            cert.handling_count += 1;
            if incident {
                cert.incident_count += 1;
            }
        }
    }

    /// Start recertification for a hazmat class
    /// Returns None if already recertifying or cert doesn't exist
    pub fn start_recertification(&mut self, hazmat_class: u8, now: u64) -> Option<CertificationEvent> {
        // Check cert exists
        if !self.certifications.contains_key(&hazmat_class) {
            return None;
        }

        // Check not already recertifying this class
        if self.pending_recertifications.iter().any(|r| r.hazmat_class == hazmat_class) {
            return None;
        }

        let action = RecertificationAction {
            hazmat_class,
            started_at: now,
            completes_at: now + RECERTIFICATION_DURATION_MICROS,
        };
        self.pending_recertifications.push(action);

        log_metrics(&MetricsEvent::RecertificationStarted {
            node_id: self.worker_id.clone(),
            hazmat_class,
            timestamp_us: now as u128,
        });

        Some(CertificationEvent::RecertificationStarted { hazmat_class })
    }

    /// Tick the certification lifecycle at the given sim_time
    /// Returns any events that occurred during this tick
    pub fn tick(&mut self, now: u64) -> Vec<CertificationEvent> {
        let mut events = Vec::new();

        // Check completed recertifications first
        let (completed, pending): (Vec<_>, Vec<_>) = self.pending_recertifications
            .drain(..)
            .partition(|r| now >= r.completes_at);

        self.pending_recertifications = pending;

        for action in completed {
            if let Some(cert) = self.certifications.get_mut(&action.hazmat_class) {
                // Restore certification
                cert.certification_valid = true;
                cert.issued_at = now;
                cert.expires_at = now + 365 * MICROS_PER_DAY; // 1 year validity
                cert.warning_emitted = false;
                cert.expiry_emitted = false;

                let confidence = cert.capability_confidence();

                log_metrics(&MetricsEvent::RecertificationCompleted {
                    node_id: self.worker_id.clone(),
                    hazmat_class: action.hazmat_class,
                    new_confidence: confidence,
                    timestamp_us: now as u128,
                });

                events.push(CertificationEvent::RecertificationCompleted {
                    hazmat_class: action.hazmat_class,
                    new_confidence: confidence,
                });
            }
        }

        // Check each certification for expiry/warning
        for cert in self.certifications.values_mut() {
            if cert.certification_valid && now >= cert.expires_at && !cert.expiry_emitted {
                // Certification has expired
                cert.certification_valid = false;
                cert.expiry_emitted = true;

                log_metrics(&MetricsEvent::CertificationExpired {
                    node_id: self.worker_id.clone(),
                    hazmat_class: cert.hazmat_class,
                    handling_count: cert.handling_count,
                    incident_count: cert.incident_count,
                    has_evidence_chain: cert.has_evidence_chain(),
                    reduced_confidence: cert.capability_confidence(),
                    timestamp_us: now as u128,
                });

                events.push(CertificationEvent::Expired {
                    hazmat_class: cert.hazmat_class,
                    handling_count: cert.handling_count,
                    incident_count: cert.incident_count,
                });
            } else if cert.certification_valid
                && !cert.warning_emitted
                && cert.expires_at > now
                && (cert.expires_at - now) <= EXPIRY_WARNING_MICROS
            {
                // Within 30-day warning window
                let days_remaining = cert.days_remaining(now);
                cert.warning_emitted = true;

                log_metrics(&MetricsEvent::CertificationExpiring {
                    node_id: self.worker_id.clone(),
                    hazmat_class: cert.hazmat_class,
                    days_remaining,
                    expires_at_us: cert.expires_at as u128,
                    timestamp_us: now as u128,
                });

                events.push(CertificationEvent::Expiring {
                    hazmat_class: cert.hazmat_class,
                    days_remaining,
                });
            }
        }

        events
    }

    /// Get the HAZMAT_HANDLING capability status for document fields
    pub fn capability_fields(&self) -> Vec<(String, f32)> {
        let mut fields = Vec::new();
        for cert in self.certifications.values() {
            let cap_name = format!("hazmat_handling:class_{}", cert.hazmat_class);
            fields.push((cap_name, cert.capability_confidence()));
        }
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        fields
    }

    /// Check if any certification is expired and not being recertified
    pub fn has_expired_certs(&self) -> bool {
        self.certifications.values().any(|c| {
            !c.certification_valid
                && !self.pending_recertifications.iter().any(|r| r.hazmat_class == c.hazmat_class)
        })
    }

    /// Get all certifications
    pub fn certifications(&self) -> &HashMap<u8, HazmatCertification> {
        &self.certifications
    }

    /// Get recertification duration in microseconds
    pub fn recertification_duration() -> u64 {
        RECERTIFICATION_DURATION_MICROS
    }
}

/// Generate initial hazmat certifications for a port-ops worker.
/// Deterministically assigns certs based on worker_id hash.
/// Some workers get certs expiring soon (for testing lifecycle).
pub fn generate_worker_certifications(
    worker_id: &str,
    sim_start_time: u64,
) -> CertificationManager {
    let mut manager = CertificationManager::new(worker_id.to_string());

    let hash: u32 = worker_id
        .chars()
        .fold(0u32, |acc, c| acc.wrapping_add(c as u32));

    // All port-ops workers get at least class 9 (misc dangerous goods)
    let class9_validity = if hash % 4 == 0 {
        // 25% of workers have certs expiring in ~20 days (within warning window)
        20
    } else {
        365
    };
    let mut cert9 = HazmatCertification::new(9, sim_start_time, class9_validity);

    // Workers with experience get handling records
    if hash % 3 == 0 {
        // ~33% are experienced handlers (like worker-2: 47 handlings, 0 incidents)
        cert9.handling_count = 40 + (hash % 20) as u64;
        cert9.incident_count = 0;
    } else if hash % 7 == 0 {
        // ~14% have some incidents
        cert9.handling_count = 10 + (hash % 15) as u64;
        cert9.incident_count = 1 + (hash % 3) as u64;
    }
    manager.add_certification(cert9);

    // ~50% of workers also certified for class 3 (flammable liquids)
    if hash % 2 == 0 {
        let class3_validity = if hash % 6 == 0 { 15 } else { 300 };
        let mut cert3 = HazmatCertification::new(3, sim_start_time, class3_validity);
        if hash % 5 == 0 {
            cert3.handling_count = 20 + (hash % 30) as u64;
            cert3.incident_count = 0;
        }
        manager.add_certification(cert3);
    }

    // ~33% also certified for class 8 (corrosives)
    if hash % 3 == 0 {
        let class8_validity = if hash % 8 == 0 { 10 } else { 250 };
        let cert8 = HazmatCertification::new(8, sim_start_time, class8_validity);
        manager.add_certification(cert8);
    }

    manager
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cert(class: u8, issued_at: u64, validity_days: u64) -> HazmatCertification {
        HazmatCertification::new(class, issued_at, validity_days)
    }

    #[test]
    fn test_certification_new() {
        let cert = make_cert(3, 0, 365);
        assert_eq!(cert.hazmat_class, 3);
        assert_eq!(cert.issued_at, 0);
        assert_eq!(cert.expires_at, 365 * MICROS_PER_DAY);
        assert!(cert.certification_valid);
        assert_eq!(cert.handling_count, 0);
        assert_eq!(cert.incident_count, 0);
    }

    #[test]
    fn test_capability_confidence_valid() {
        let cert = make_cert(9, 0, 365);
        assert_eq!(cert.capability_confidence(), 1.0);
    }

    #[test]
    fn test_capability_confidence_expired_no_evidence() {
        let mut cert = make_cert(9, 0, 365);
        cert.certification_valid = false;
        // No handling count = no evidence chain
        assert_eq!(cert.capability_confidence(), 0.0);
    }

    #[test]
    fn test_capability_confidence_expired_with_evidence_chain() {
        let mut cert = make_cert(9, 0, 365);
        cert.certification_valid = false;
        cert.handling_count = 47;
        cert.incident_count = 0;
        assert_eq!(cert.capability_confidence(), 0.7);
    }

    #[test]
    fn test_capability_confidence_expired_with_incidents() {
        let mut cert = make_cert(9, 0, 365);
        cert.certification_valid = false;
        cert.handling_count = 47;
        cert.incident_count = 2;
        // Has incidents, so no evidence chain → 0.0
        assert_eq!(cert.capability_confidence(), 0.0);
    }

    #[test]
    fn test_days_remaining() {
        let cert = make_cert(9, 0, 365);
        assert_eq!(cert.days_remaining(0), 365);
        assert_eq!(cert.days_remaining(100 * MICROS_PER_DAY), 265);
        assert_eq!(cert.days_remaining(365 * MICROS_PER_DAY), 0);
        assert_eq!(cert.days_remaining(400 * MICROS_PER_DAY), 0);
    }

    #[test]
    fn test_expiry_warning_at_30_days() {
        let mut manager = CertificationManager::new("worker-1".to_string());
        manager.add_certification(make_cert(9, 0, 60)); // expires in 60 days

        // At day 0: no events (60 days remaining > 30)
        let events = manager.tick(0);
        assert!(events.is_empty());

        // At day 29: still no warning (31 days remaining > 30)
        let events = manager.tick(29 * MICROS_PER_DAY);
        assert!(events.is_empty());

        // At day 30: warning (30 days remaining == 30)
        let events = manager.tick(30 * MICROS_PER_DAY);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], CertificationEvent::Expiring { hazmat_class: 9, days_remaining: 30 }));

        // At day 31: no duplicate warning
        let events = manager.tick(31 * MICROS_PER_DAY);
        assert!(events.is_empty());
    }

    #[test]
    fn test_certification_expires() {
        let mut manager = CertificationManager::new("worker-2".to_string());
        let mut cert = make_cert(9, 0, 60);
        cert.handling_count = 47;
        cert.incident_count = 0;
        cert.warning_emitted = true; // Skip warning for this test
        manager.add_certification(cert);

        // Before expiry: no events
        let events = manager.tick(59 * MICROS_PER_DAY);
        assert!(events.is_empty());

        // At expiry: expired event
        let events = manager.tick(60 * MICROS_PER_DAY);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            CertificationEvent::Expired {
                hazmat_class: 9,
                handling_count: 47,
                incident_count: 0,
            }
        ));

        // Verify capability downgraded
        let fields = manager.capability_fields();
        let class9 = fields.iter().find(|(name, _)| name == "hazmat_handling:class_9").unwrap();
        assert_eq!(class9.1, 0.7); // Evidence chain: 47 handlings, 0 incidents

        // No duplicate expiry event
        let events = manager.tick(61 * MICROS_PER_DAY);
        assert!(events.is_empty());
    }

    #[test]
    fn test_recertification_flow() {
        let mut manager = CertificationManager::new("worker-3".to_string());
        let mut cert = make_cert(9, 0, 30);
        cert.warning_emitted = true;
        cert.expiry_emitted = true;
        cert.certification_valid = false;
        manager.add_certification(cert);

        let now = 31 * MICROS_PER_DAY;

        // Start recertification
        let event = manager.start_recertification(9, now);
        assert!(matches!(event, Some(CertificationEvent::RecertificationStarted { hazmat_class: 9 })));

        // Before completion: no events
        let events = manager.tick(now + RECERTIFICATION_DURATION_MICROS - 1);
        assert!(events.is_empty());

        // At completion: recertification complete
        let events = manager.tick(now + RECERTIFICATION_DURATION_MICROS);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            CertificationEvent::RecertificationCompleted {
                hazmat_class: 9,
                new_confidence,
            } if new_confidence == 1.0
        ));

        // Capability restored
        let fields = manager.capability_fields();
        let class9 = fields.iter().find(|(name, _)| name == "hazmat_handling:class_9").unwrap();
        assert_eq!(class9.1, 1.0);
    }

    #[test]
    fn test_cannot_double_recertify() {
        let mut manager = CertificationManager::new("worker-4".to_string());
        let mut cert = make_cert(9, 0, 30);
        cert.certification_valid = false;
        cert.expiry_emitted = true;
        manager.add_certification(cert);

        let now = 31 * MICROS_PER_DAY;
        assert!(manager.start_recertification(9, now).is_some());
        assert!(manager.start_recertification(9, now).is_none()); // Already in progress
    }

    #[test]
    fn test_recertification_nonexistent_class() {
        let mut manager = CertificationManager::new("worker-5".to_string());
        assert!(manager.start_recertification(3, 0).is_none()); // No class 3 cert
    }

    #[test]
    fn test_has_expired_certs() {
        let mut manager = CertificationManager::new("worker-6".to_string());
        let mut cert = make_cert(9, 0, 10);
        cert.warning_emitted = true;
        manager.add_certification(cert);

        // Before expiry
        manager.tick(5 * MICROS_PER_DAY);
        assert!(!manager.has_expired_certs());

        // After expiry
        manager.tick(11 * MICROS_PER_DAY);
        assert!(manager.has_expired_certs());

        // Start recertification - no longer counts as "expired needing attention"
        manager.start_recertification(9, 12 * MICROS_PER_DAY);
        assert!(!manager.has_expired_certs());
    }

    #[test]
    fn test_record_handling() {
        let mut manager = CertificationManager::new("worker-7".to_string());
        manager.add_certification(make_cert(9, 0, 365));

        manager.record_handling(9, false);
        manager.record_handling(9, false);
        manager.record_handling(9, true);

        let cert = &manager.certifications()[&9];
        assert_eq!(cert.handling_count, 3);
        assert_eq!(cert.incident_count, 1);
    }

    #[test]
    fn test_multiple_classes() {
        let mut manager = CertificationManager::new("worker-8".to_string());
        manager.add_certification(make_cert(3, 0, 20)); // Expires in 20 days (within warning)
        manager.add_certification(make_cert(9, 0, 365)); // Expires in 365 days (no warning)

        // Tick: should get warning for class 3 only
        let events = manager.tick(0);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], CertificationEvent::Expiring { hazmat_class: 3, .. }));
    }

    #[test]
    fn test_generate_worker_certifications() {
        let manager = generate_worker_certifications("worker-test", 0);
        // Every worker gets at least class 9
        assert!(manager.certifications().contains_key(&9));
    }

    #[test]
    fn test_capability_fields_sorted() {
        let mut manager = CertificationManager::new("worker-9".to_string());
        manager.add_certification(make_cert(9, 0, 365));
        manager.add_certification(make_cert(3, 0, 365));
        manager.add_certification(make_cert(8, 0, 365));

        let fields = manager.capability_fields();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].0, "hazmat_handling:class_3");
        assert_eq!(fields[1].0, "hazmat_handling:class_8");
        assert_eq!(fields[2].0, "hazmat_handling:class_9");
        // All valid = 1.0 confidence
        for (_, confidence) in &fields {
            assert_eq!(*confidence, 1.0);
        }
    }
}
