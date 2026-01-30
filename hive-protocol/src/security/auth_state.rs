//! Authentication state machine for certificate expiration tracking.
//!
//! Implements graceful degradation per ADR-048:
//! - Valid → Warning → GracePeriod → Expired
//! - Configurable intervals and thresholds

use super::MembershipCertificate;

/// Certificate validity state with remaining time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateState {
    /// Certificate is valid with time remaining until expiration.
    Valid {
        /// Milliseconds until expiration.
        expires_in_ms: u64,
    },
    /// Certificate approaching expiration (within warning threshold).
    Warning {
        /// Milliseconds until expiration.
        expires_in_ms: u64,
    },
    /// Certificate expired but within grace period.
    GracePeriod {
        /// Milliseconds remaining in grace period.
        grace_remaining_ms: u64,
    },
    /// Certificate expired and grace period exhausted.
    Expired,
}

impl CertificateState {
    /// Returns true if the certificate allows mesh operations.
    pub fn is_operational(&self) -> bool {
        matches!(
            self,
            CertificateState::Valid { .. }
                | CertificateState::Warning { .. }
                | CertificateState::GracePeriod { .. }
        )
    }

    /// Returns true if re-authentication should be initiated.
    pub fn should_reauth(&self) -> bool {
        matches!(
            self,
            CertificateState::Warning { .. } | CertificateState::GracePeriod { .. }
        )
    }

    /// Returns true if the certificate is fully expired.
    pub fn is_expired(&self) -> bool {
        matches!(self, CertificateState::Expired)
    }
}

/// Configuration for authentication intervals and thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthConfig {
    /// Certificate validity period in hours. Default: 24.
    pub auth_interval_hours: u16,
    /// Grace period after expiration in hours. Default: 4.
    pub grace_period_hours: u16,
    /// Warning threshold before expiration in hours. Default: 1.
    pub warning_threshold_hours: u16,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auth_interval_hours: 24,
            grace_period_hours: 4,
            warning_threshold_hours: 1,
        }
    }
}

impl AuthConfig {
    /// Create config with custom intervals.
    pub fn new(
        auth_interval_hours: u16,
        grace_period_hours: u16,
        warning_threshold_hours: u16,
    ) -> Self {
        Self {
            auth_interval_hours,
            grace_period_hours,
            warning_threshold_hours,
        }
    }

    /// Auth interval in milliseconds.
    pub fn auth_interval_ms(&self) -> u64 {
        self.auth_interval_hours as u64 * 3_600_000
    }

    /// Grace period in milliseconds.
    pub fn grace_period_ms(&self) -> u64 {
        self.grace_period_hours as u64 * 3_600_000
    }

    /// Warning threshold in milliseconds.
    pub fn warning_threshold_ms(&self) -> u64 {
        self.warning_threshold_hours as u64 * 3_600_000
    }
}

/// Tracks authentication state for membership certificates.
#[derive(Debug, Clone)]
pub struct AuthStateTracker {
    config: AuthConfig,
}

impl Default for AuthStateTracker {
    fn default() -> Self {
        Self::new(AuthConfig::default())
    }
}

impl AuthStateTracker {
    /// Create a new tracker with the given configuration.
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// Check the state of a certificate at the given time.
    ///
    /// # Timeline
    /// ```text
    /// ├── T-0: Certificate issued
    /// ├── T-(auth_interval - warning_threshold): Warning state
    /// ├── T-auth_interval: Expiration (grace period starts)
    /// ├── T-(auth_interval + grace_period): Hard cutoff (Expired)
    /// └── Re-auth succeeds: New cert, timer resets
    /// ```
    pub fn check_state(&self, cert: &MembershipCertificate, now_ms: u64) -> CertificateState {
        let expires_at = cert.expires_at_ms;

        if now_ms < expires_at {
            // Certificate has not yet expired
            let expires_in_ms = expires_at - now_ms;

            if expires_in_ms <= self.config.warning_threshold_ms() {
                CertificateState::Warning { expires_in_ms }
            } else {
                CertificateState::Valid { expires_in_ms }
            }
        } else {
            // Certificate has expired
            let expired_for_ms = now_ms - expires_at;

            if expired_for_ms < self.config.grace_period_ms() {
                let grace_remaining_ms = self.config.grace_period_ms() - expired_for_ms;
                CertificateState::GracePeriod { grace_remaining_ms }
            } else {
                CertificateState::Expired
            }
        }
    }

    /// Check if a certificate needs re-authentication.
    ///
    /// Returns true if the certificate is in Warning or GracePeriod state.
    pub fn needs_reauth(&self, cert: &MembershipCertificate, now_ms: u64) -> bool {
        self.check_state(cert, now_ms).should_reauth()
    }

    /// Check if a certificate is still operational (allows mesh operations).
    ///
    /// Returns true for Valid, Warning, and GracePeriod states.
    pub fn is_operational(&self, cert: &MembershipCertificate, now_ms: u64) -> bool {
        self.check_state(cert, now_ms).is_operational()
    }

    /// Check if a certificate has fully expired (no grace period remaining).
    pub fn is_expired(&self, cert: &MembershipCertificate, now_ms: u64) -> bool {
        self.check_state(cert, now_ms).is_expired()
    }

    /// Calculate when re-authentication should begin (warning threshold).
    pub fn reauth_deadline(&self, cert: &MembershipCertificate) -> u64 {
        cert.expires_at_ms
            .saturating_sub(self.config.warning_threshold_ms())
    }

    /// Calculate the hard cutoff time (grace period exhausted).
    pub fn hard_cutoff(&self, cert: &MembershipCertificate) -> u64 {
        cert.expires_at_ms
            .saturating_add(self.config.grace_period_ms())
    }
}

/// Event emitted on state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStateEvent {
    /// Transitioned from Valid to Warning.
    EnteringWarning { expires_in_ms: u64 },
    /// Transitioned from Warning to GracePeriod.
    EnteringGracePeriod { grace_remaining_ms: u64 },
    /// Transitioned to Expired (hard cutoff).
    Expired,
    /// Re-authenticated successfully, back to Valid.
    Renewed { new_expires_at_ms: u64 },
}

/// Monitors certificates and emits state transition events.
#[derive(Debug, Clone)]
pub struct AuthStateMonitor {
    tracker: AuthStateTracker,
    last_state: Option<CertificateState>,
}

impl AuthStateMonitor {
    /// Create a new monitor with the given tracker.
    pub fn new(tracker: AuthStateTracker) -> Self {
        Self {
            tracker,
            last_state: None,
        }
    }

    /// Update the monitor with current time and check for state transitions.
    ///
    /// Returns an event if the state changed.
    pub fn update(&mut self, cert: &MembershipCertificate, now_ms: u64) -> Option<AuthStateEvent> {
        let new_state = self.tracker.check_state(cert, now_ms);

        let event = match (&self.last_state, &new_state) {
            // Valid → Warning
            (Some(CertificateState::Valid { .. }), CertificateState::Warning { expires_in_ms })
            | (None, CertificateState::Warning { expires_in_ms }) => {
                Some(AuthStateEvent::EnteringWarning {
                    expires_in_ms: *expires_in_ms,
                })
            }

            // Warning → GracePeriod
            (
                Some(CertificateState::Warning { .. }),
                CertificateState::GracePeriod { grace_remaining_ms },
            ) => Some(AuthStateEvent::EnteringGracePeriod {
                grace_remaining_ms: *grace_remaining_ms,
            }),

            // Any → Expired
            (Some(state), CertificateState::Expired) if *state != CertificateState::Expired => {
                Some(AuthStateEvent::Expired)
            }

            _ => None,
        };

        self.last_state = Some(new_state);
        event
    }

    /// Notify the monitor that re-authentication succeeded.
    ///
    /// Call this after a new certificate is obtained.
    pub fn notify_renewed(&mut self, new_cert: &MembershipCertificate) -> AuthStateEvent {
        self.last_state = Some(CertificateState::Valid {
            expires_in_ms: new_cert.expires_at_ms,
        });
        AuthStateEvent::Renewed {
            new_expires_at_ms: new_cert.expires_at_ms,
        }
    }

    /// Get the current state without triggering events.
    pub fn current_state(&self) -> Option<&CertificateState> {
        self.last_state.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a test certificate
    fn test_cert(issued_at_ms: u64, expires_at_ms: u64) -> MembershipCertificate {
        MembershipCertificate {
            member_public_key: [0u8; 32],
            mesh_id: "A1B2C3D4".to_string(),
            callsign: "TEST-01".to_string(),
            permissions: super::super::MemberPermissions::STANDARD,
            issued_at_ms,
            expires_at_ms,
            issuer_public_key: [0u8; 32],
            issuer_signature: [0u8; 64],
        }
    }

    #[test]
    fn test_config_defaults() {
        let config = AuthConfig::default();
        assert_eq!(config.auth_interval_hours, 24);
        assert_eq!(config.grace_period_hours, 4);
        assert_eq!(config.warning_threshold_hours, 1);
    }

    #[test]
    fn test_config_to_ms() {
        let config = AuthConfig::default();
        assert_eq!(config.auth_interval_ms(), 24 * 3_600_000);
        assert_eq!(config.grace_period_ms(), 4 * 3_600_000);
        assert_eq!(config.warning_threshold_ms(), 3_600_000);
    }

    #[test]
    fn test_valid_state() {
        let tracker = AuthStateTracker::default();
        let cert = test_cert(0, 24 * 3_600_000); // Expires in 24h

        // At T=0, 24h remaining
        let state = tracker.check_state(&cert, 0);
        assert!(
            matches!(state, CertificateState::Valid { expires_in_ms } if expires_in_ms == 24 * 3_600_000)
        );
        assert!(state.is_operational());
        assert!(!state.should_reauth());

        // At T=12h, 12h remaining
        let state = tracker.check_state(&cert, 12 * 3_600_000);
        assert!(
            matches!(state, CertificateState::Valid { expires_in_ms } if expires_in_ms == 12 * 3_600_000)
        );
    }

    #[test]
    fn test_warning_state() {
        let tracker = AuthStateTracker::default();
        let cert = test_cert(0, 24 * 3_600_000);

        // At T=23h, 1h remaining (within warning threshold)
        let state = tracker.check_state(&cert, 23 * 3_600_000);
        assert!(
            matches!(state, CertificateState::Warning { expires_in_ms } if expires_in_ms == 3_600_000)
        );
        assert!(state.is_operational());
        assert!(state.should_reauth());

        // At T=23.5h, 30min remaining
        let state = tracker.check_state(&cert, 23 * 3_600_000 + 1_800_000);
        assert!(
            matches!(state, CertificateState::Warning { expires_in_ms } if expires_in_ms == 1_800_000)
        );
    }

    #[test]
    fn test_grace_period_state() {
        let tracker = AuthStateTracker::default();
        let cert = test_cert(0, 24 * 3_600_000);

        // At T=24h, just expired (4h grace remaining)
        let state = tracker.check_state(&cert, 24 * 3_600_000);
        assert!(
            matches!(state, CertificateState::GracePeriod { grace_remaining_ms } if grace_remaining_ms == 4 * 3_600_000)
        );
        assert!(state.is_operational());
        assert!(state.should_reauth());

        // At T=26h, 2h into grace (2h remaining)
        let state = tracker.check_state(&cert, 26 * 3_600_000);
        assert!(
            matches!(state, CertificateState::GracePeriod { grace_remaining_ms } if grace_remaining_ms == 2 * 3_600_000)
        );
    }

    #[test]
    fn test_expired_state() {
        let tracker = AuthStateTracker::default();
        let cert = test_cert(0, 24 * 3_600_000);

        // At T=28h, grace period exhausted
        let state = tracker.check_state(&cert, 28 * 3_600_000);
        assert!(matches!(state, CertificateState::Expired));
        assert!(!state.is_operational());
        assert!(!state.should_reauth()); // Too late to reauth

        // At T=30h, still expired
        let state = tracker.check_state(&cert, 30 * 3_600_000);
        assert!(matches!(state, CertificateState::Expired));
    }

    #[test]
    fn test_needs_reauth() {
        let tracker = AuthStateTracker::default();
        let cert = test_cert(0, 24 * 3_600_000);

        // Valid: no reauth needed
        assert!(!tracker.needs_reauth(&cert, 0));
        assert!(!tracker.needs_reauth(&cert, 22 * 3_600_000));

        // Warning: reauth needed
        assert!(tracker.needs_reauth(&cert, 23 * 3_600_000));
        assert!(tracker.needs_reauth(&cert, 23 * 3_600_000 + 1_800_000));

        // Grace period: reauth needed
        assert!(tracker.needs_reauth(&cert, 25 * 3_600_000));

        // Expired: too late
        assert!(!tracker.needs_reauth(&cert, 29 * 3_600_000));
    }

    #[test]
    fn test_is_operational() {
        let tracker = AuthStateTracker::default();
        let cert = test_cert(0, 24 * 3_600_000);

        assert!(tracker.is_operational(&cert, 0));
        assert!(tracker.is_operational(&cert, 23 * 3_600_000)); // Warning
        assert!(tracker.is_operational(&cert, 26 * 3_600_000)); // Grace
        assert!(!tracker.is_operational(&cert, 29 * 3_600_000)); // Expired
    }

    #[test]
    fn test_deadlines() {
        let tracker = AuthStateTracker::default();
        let cert = test_cert(0, 24 * 3_600_000);

        // Reauth deadline = expires - warning_threshold = 24h - 1h = 23h
        assert_eq!(tracker.reauth_deadline(&cert), 23 * 3_600_000);

        // Hard cutoff = expires + grace_period = 24h + 4h = 28h
        assert_eq!(tracker.hard_cutoff(&cert), 28 * 3_600_000);
    }

    #[test]
    fn test_custom_config() {
        let config = AuthConfig::new(48, 8, 2); // 48h validity, 8h grace, 2h warning
        let tracker = AuthStateTracker::new(config);
        let cert = test_cert(0, 48 * 3_600_000);

        // At T=45h, still valid (3h remaining, warning at 2h)
        let state = tracker.check_state(&cert, 45 * 3_600_000);
        assert!(matches!(state, CertificateState::Valid { .. }));

        // At T=46.5h, warning (1.5h remaining)
        let state = tracker.check_state(&cert, 46 * 3_600_000 + 1_800_000);
        assert!(matches!(state, CertificateState::Warning { .. }));

        // At T=52h, grace period (4h into 8h grace)
        let state = tracker.check_state(&cert, 52 * 3_600_000);
        assert!(
            matches!(state, CertificateState::GracePeriod { grace_remaining_ms } if grace_remaining_ms == 4 * 3_600_000)
        );

        // At T=56h, expired (grace exhausted)
        let state = tracker.check_state(&cert, 56 * 3_600_000);
        assert!(matches!(state, CertificateState::Expired));
    }

    #[test]
    fn test_monitor_transitions() {
        let tracker = AuthStateTracker::default();
        let mut monitor = AuthStateMonitor::new(tracker);
        let cert = test_cert(0, 24 * 3_600_000);

        // Initial check at T=0 (Valid)
        let event = monitor.update(&cert, 0);
        assert!(event.is_none()); // No transition from None → Valid

        // At T=22h, still valid
        let event = monitor.update(&cert, 22 * 3_600_000);
        assert!(event.is_none());

        // At T=23h, Valid → Warning
        let event = monitor.update(&cert, 23 * 3_600_000);
        assert!(matches!(
            event,
            Some(AuthStateEvent::EnteringWarning { .. })
        ));

        // At T=24h, Warning → GracePeriod
        let event = monitor.update(&cert, 24 * 3_600_000);
        assert!(matches!(
            event,
            Some(AuthStateEvent::EnteringGracePeriod { .. })
        ));

        // At T=28h, GracePeriod → Expired
        let event = monitor.update(&cert, 28 * 3_600_000);
        assert!(matches!(event, Some(AuthStateEvent::Expired)));

        // At T=30h, still Expired (no new event)
        let event = monitor.update(&cert, 30 * 3_600_000);
        assert!(event.is_none());
    }

    #[test]
    fn test_monitor_renewal() {
        let tracker = AuthStateTracker::default();
        let mut monitor = AuthStateMonitor::new(tracker);
        let cert = test_cert(0, 24 * 3_600_000);

        // Get into warning state
        monitor.update(&cert, 23 * 3_600_000);

        // Simulate re-auth with new cert
        let new_cert = test_cert(23 * 3_600_000, 47 * 3_600_000);
        let event = monitor.notify_renewed(&new_cert);
        assert!(matches!(
            event,
            AuthStateEvent::Renewed {
                new_expires_at_ms: exp
            } if exp == 47 * 3_600_000
        ));

        // Now monitoring the new cert, should be valid
        let state = monitor.current_state();
        assert!(matches!(state, Some(CertificateState::Valid { .. })));
    }
}
