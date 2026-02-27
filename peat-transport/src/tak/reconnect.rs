//! Reconnection manager with exponential backoff

use rand::Rng;
use std::time::{Duration, Instant};

use super::config::ReconnectPolicy;

/// Reconnection state machine with exponential backoff
pub struct ReconnectionManager {
    policy: ReconnectPolicy,
    current_delay: Duration,
    attempts: usize,
    last_attempt: Option<Instant>,
}

impl ReconnectionManager {
    /// Create a new reconnection manager with the given policy
    pub fn new(policy: ReconnectPolicy) -> Self {
        Self {
            current_delay: policy.initial_delay,
            policy,
            attempts: 0,
            last_attempt: None,
        }
    }

    /// Check if we should attempt reconnection
    pub fn should_reconnect(&self) -> bool {
        if !self.policy.enabled {
            return false;
        }

        if let Some(max) = self.policy.max_attempts {
            if self.attempts >= max {
                return false;
            }
        }

        true
    }

    /// Get the next delay before reconnection attempt
    ///
    /// Implements exponential backoff with jitter.
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current_delay;

        // Apply exponential backoff
        let next_delay_secs = (self.current_delay.as_secs_f64() * self.policy.backoff_multiplier)
            .min(self.policy.max_delay.as_secs_f64());
        self.current_delay = Duration::from_secs_f64(next_delay_secs);

        // Apply jitter
        let jitter_range = delay.as_secs_f64() * self.policy.jitter;
        let jitter = if jitter_range > 0.0 {
            rand::thread_rng().gen_range(-jitter_range..jitter_range)
        } else {
            0.0
        };
        let final_delay = Duration::from_secs_f64((delay.as_secs_f64() + jitter).max(0.0));

        self.attempts += 1;
        self.last_attempt = Some(Instant::now());

        final_delay
    }

    /// Reset the reconnection state after a successful connection
    pub fn reset(&mut self) {
        self.current_delay = self.policy.initial_delay;
        self.attempts = 0;
    }

    /// Get the number of reconnection attempts
    pub fn attempts(&self) -> usize {
        self.attempts
    }

    /// Get time since last attempt
    pub fn time_since_last_attempt(&self) -> Option<Duration> {
        self.last_attempt.map(|t| t.elapsed())
    }

    /// Check if we've exhausted all attempts
    pub fn is_exhausted(&self) -> bool {
        if let Some(max) = self.policy.max_attempts {
            self.attempts >= max
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = ReconnectPolicy::default();
        let mut manager = ReconnectionManager::new(policy);

        assert!(manager.should_reconnect());
        assert_eq!(manager.attempts(), 0);

        let delay1 = manager.next_delay();
        assert!(delay1 >= Duration::from_millis(900)); // ~1s with jitter
        assert!(delay1 <= Duration::from_millis(1100));
        assert_eq!(manager.attempts(), 1);
    }

    #[test]
    fn test_exponential_backoff() {
        let policy = ReconnectPolicy {
            enabled: true,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: None,
            jitter: 0.0, // Disable jitter for predictable testing
        };
        let mut manager = ReconnectionManager::new(policy);

        assert_eq!(manager.next_delay(), Duration::from_secs(1));
        assert_eq!(manager.next_delay(), Duration::from_secs(2));
        assert_eq!(manager.next_delay(), Duration::from_secs(4));
        assert_eq!(manager.next_delay(), Duration::from_secs(8));
    }

    #[test]
    fn test_max_delay_cap() {
        let policy = ReconnectPolicy {
            enabled: true,
            initial_delay: Duration::from_secs(30),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: None,
            jitter: 0.0,
        };
        let mut manager = ReconnectionManager::new(policy);

        assert_eq!(manager.next_delay(), Duration::from_secs(30));
        assert_eq!(manager.next_delay(), Duration::from_secs(60)); // Capped
        assert_eq!(manager.next_delay(), Duration::from_secs(60)); // Still capped
    }

    #[test]
    fn test_max_attempts() {
        let policy = ReconnectPolicy {
            enabled: true,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: Some(3),
            jitter: 0.0,
        };
        let mut manager = ReconnectionManager::new(policy);

        assert!(manager.should_reconnect());
        manager.next_delay();
        assert!(manager.should_reconnect());
        manager.next_delay();
        assert!(manager.should_reconnect());
        manager.next_delay();
        assert!(!manager.should_reconnect()); // Exhausted
        assert!(manager.is_exhausted());
    }

    #[test]
    fn test_reset() {
        let policy = ReconnectPolicy {
            enabled: true,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            max_attempts: Some(3),
            jitter: 0.0,
        };
        let mut manager = ReconnectionManager::new(policy);

        manager.next_delay();
        manager.next_delay();
        assert_eq!(manager.attempts(), 2);

        manager.reset();
        assert_eq!(manager.attempts(), 0);
        assert!(manager.should_reconnect());
        assert_eq!(manager.next_delay(), Duration::from_secs(1));
    }

    #[test]
    fn test_disabled_policy() {
        let policy = ReconnectPolicy {
            enabled: false,
            ..Default::default()
        };
        let manager = ReconnectionManager::new(policy);

        assert!(!manager.should_reconnect());
    }
}
