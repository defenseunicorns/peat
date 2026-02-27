//! Timeout management for hierarchical commands
//!
//! Handles command expiration (TTL) and acknowledgment timeout tracking.

use crate::error::Result;
use peat_schema::command::v1::HierarchicalCommand;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

/// Acknowledgment timeout tracking
#[derive(Debug, Clone)]
pub struct AckTimeout {
    /// Command ID being tracked
    pub command_id: String,
    /// Node IDs expected to acknowledge
    pub expected_acks: Vec<String>,
    /// Node IDs that have acknowledged
    pub received_acks: Vec<String>,
    /// Time when timeout expires
    pub expires_at: SystemTime,
}

/// Timeout manager for commands and acknowledgments
///
/// Tracks command expiration (TTL) and acknowledgment timeouts.
/// Provides efficient lookup of expired commands via BTreeMap.
pub struct TimeoutManager {
    /// Commands indexed by expiration time
    /// Key: expiration time, Value: list of command IDs expiring at that time
    expiring_commands: Arc<RwLock<BTreeMap<SystemTime, Vec<String>>>>,

    /// Acknowledgment timeout tracking
    /// Key: command_id, Value: acknowledgment timeout info
    ack_timeouts: Arc<RwLock<HashMap<String, AckTimeout>>>,
}

impl TimeoutManager {
    /// Create a new timeout manager
    pub fn new() -> Self {
        Self {
            expiring_commands: Arc::new(RwLock::new(BTreeMap::new())),
            ack_timeouts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a command for expiration tracking
    ///
    /// If the command has an `expires_at` field, it will be tracked
    /// for automatic expiration.
    pub async fn register_expiration(&self, command: &HierarchicalCommand) -> Result<()> {
        if let Some(expires_at) = command.expires_at.as_ref() {
            let expiry = SystemTime::UNIX_EPOCH + Duration::from_secs(expires_at.seconds);
            self.expiring_commands
                .write()
                .await
                .entry(expiry)
                .or_default()
                .push(command.command_id.clone());
        }
        Ok(())
    }

    /// Check and process expired commands
    ///
    /// Returns a list of command IDs that have expired.
    /// This should be called periodically by a background task.
    pub async fn process_expired(&self) -> Vec<String> {
        let now = SystemTime::now();
        let mut expired = Vec::new();

        let mut expiring = self.expiring_commands.write().await;

        // Collect all expired keys (expiration times <= now)
        let expired_keys: Vec<SystemTime> = expiring.range(..=now).map(|(k, _)| *k).collect();

        // Remove and collect all expired commands
        for key in expired_keys {
            if let Some(commands) = expiring.remove(&key) {
                expired.extend(commands);
            }
        }

        expired
    }

    /// Unregister a command from expiration tracking
    ///
    /// Called when a command completes before expiring.
    pub async fn unregister_expiration(&self, command_id: &str) -> Result<()> {
        let mut expiring = self.expiring_commands.write().await;

        // Remove command from all expiration time buckets
        for (_, cmd_list) in expiring.iter_mut() {
            cmd_list.retain(|id| id != command_id);
        }

        // Clean up empty time buckets
        expiring.retain(|_, cmd_list| !cmd_list.is_empty());

        Ok(())
    }

    /// Register an acknowledgment timeout
    ///
    /// Tracks expected acknowledgments for a command with a timeout.
    pub async fn register_ack_timeout(
        &self,
        command_id: String,
        expected_acks: Vec<String>,
        timeout: Duration,
    ) -> Result<()> {
        let ack_timeout = AckTimeout {
            command_id: command_id.clone(),
            expected_acks,
            received_acks: Vec::new(),
            expires_at: SystemTime::now() + timeout,
        };

        self.ack_timeouts
            .write()
            .await
            .insert(command_id, ack_timeout);

        Ok(())
    }

    /// Record a received acknowledgment
    ///
    /// Updates the tracking for a command's acknowledgments.
    /// Returns true if all expected acks have been received.
    pub async fn record_ack(&self, command_id: &str, node_id: &str) -> bool {
        let mut timeouts = self.ack_timeouts.write().await;

        if let Some(timeout) = timeouts.get_mut(command_id) {
            if !timeout.received_acks.contains(&node_id.to_string()) {
                timeout.received_acks.push(node_id.to_string());
            }

            // Check if all expected acks received
            timeout.received_acks.len() >= timeout.expected_acks.len()
        } else {
            false
        }
    }

    /// Check for acknowledgment timeouts
    ///
    /// Returns list of command IDs that have timed out waiting for acks.
    /// A command has timed out if:
    /// 1. The timeout period has elapsed
    /// 2. Not all expected acknowledgments have been received
    pub async fn check_ack_timeouts(&self) -> Vec<String> {
        let now = SystemTime::now();
        let timeouts = self.ack_timeouts.read().await;

        timeouts
            .iter()
            .filter(|(_, t)| t.expires_at <= now && t.received_acks.len() < t.expected_acks.len())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get acknowledgment status for a command
    ///
    /// Returns the acknowledgment tracking info if it exists.
    pub async fn get_ack_status(&self, command_id: &str) -> Option<AckTimeout> {
        self.ack_timeouts.read().await.get(command_id).cloned()
    }

    /// Remove acknowledgment timeout tracking
    ///
    /// Called when a command completes or is cancelled.
    pub async fn unregister_ack_timeout(&self, command_id: &str) -> Result<()> {
        self.ack_timeouts.write().await.remove(command_id);
        Ok(())
    }

    /// Get count of commands being tracked for expiration
    pub async fn expiration_count(&self) -> usize {
        self.expiring_commands
            .read()
            .await
            .values()
            .map(|v| v.len())
            .sum()
    }

    /// Get count of commands being tracked for ack timeout
    pub async fn ack_timeout_count(&self) -> usize {
        self.ack_timeouts.read().await.len()
    }
}

impl Default for TimeoutManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peat_schema::command::v1::{command_target::Scope, CommandTarget};
    use peat_schema::common::v1::Timestamp;
    use tokio::time::sleep;

    fn create_test_command_with_ttl(
        command_id: &str,
        expires_at_seconds: u64,
    ) -> HierarchicalCommand {
        HierarchicalCommand {
            command_id: command_id.to_string(),
            originator_id: "test-node".to_string(),
            target: Some(CommandTarget {
                scope: Scope::Individual as i32,
                target_ids: vec!["target-1".to_string()],
            }),
            expires_at: Some(Timestamp {
                seconds: expires_at_seconds,
                nanos: 0,
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_register_and_process_expired() {
        let manager = TimeoutManager::new();

        // Create command that expires in the past
        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expired_cmd = create_test_command_with_ttl("cmd-1", now_secs - 10);

        manager.register_expiration(&expired_cmd).await.unwrap();

        // Process expired commands
        let expired = manager.process_expired().await;

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0], "cmd-1");

        // Verify count updated
        assert_eq!(manager.expiration_count().await, 0);
    }

    #[tokio::test]
    async fn test_command_not_expired_yet() {
        let manager = TimeoutManager::new();

        // Create command that expires in the future
        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let future_cmd = create_test_command_with_ttl("cmd-1", now_secs + 3600);

        manager.register_expiration(&future_cmd).await.unwrap();

        // Process expired - should be empty
        let expired = manager.process_expired().await;

        assert_eq!(expired.len(), 0);
        assert_eq!(manager.expiration_count().await, 1);
    }

    #[tokio::test]
    async fn test_unregister_expiration() {
        let manager = TimeoutManager::new();

        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cmd = create_test_command_with_ttl("cmd-1", now_secs + 3600);

        manager.register_expiration(&cmd).await.unwrap();
        assert_eq!(manager.expiration_count().await, 1);

        manager.unregister_expiration("cmd-1").await.unwrap();
        assert_eq!(manager.expiration_count().await, 0);
    }

    #[tokio::test]
    async fn test_ack_timeout_registration() {
        let manager = TimeoutManager::new();

        manager
            .register_ack_timeout(
                "cmd-1".to_string(),
                vec!["node-1".to_string(), "node-2".to_string()],
                Duration::from_secs(30),
            )
            .await
            .unwrap();

        let status = manager.get_ack_status("cmd-1").await.unwrap();
        assert_eq!(status.command_id, "cmd-1");
        assert_eq!(status.expected_acks.len(), 2);
        assert_eq!(status.received_acks.len(), 0);
    }

    #[tokio::test]
    async fn test_record_ack() {
        let manager = TimeoutManager::new();

        manager
            .register_ack_timeout(
                "cmd-1".to_string(),
                vec!["node-1".to_string(), "node-2".to_string()],
                Duration::from_secs(30),
            )
            .await
            .unwrap();

        // Record first ack
        let all_received = manager.record_ack("cmd-1", "node-1").await;
        assert!(!all_received);

        // Record second ack
        let all_received = manager.record_ack("cmd-1", "node-2").await;
        assert!(all_received);

        let status = manager.get_ack_status("cmd-1").await.unwrap();
        assert_eq!(status.received_acks.len(), 2);
    }

    #[tokio::test]
    async fn test_ack_timeout_detection() {
        let manager = TimeoutManager::new();

        // Register with very short timeout
        manager
            .register_ack_timeout(
                "cmd-1".to_string(),
                vec!["node-1".to_string(), "node-2".to_string()],
                Duration::from_millis(100),
            )
            .await
            .unwrap();

        // Only record one ack
        manager.record_ack("cmd-1", "node-1").await;

        // Wait for timeout
        sleep(Duration::from_millis(150)).await;

        // Check for timeouts
        let timed_out = manager.check_ack_timeouts().await;
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], "cmd-1");
    }

    #[tokio::test]
    async fn test_ack_timeout_not_detected_if_all_received() {
        let manager = TimeoutManager::new();

        manager
            .register_ack_timeout(
                "cmd-1".to_string(),
                vec!["node-1".to_string(), "node-2".to_string()],
                Duration::from_millis(100),
            )
            .await
            .unwrap();

        // Record all acks
        manager.record_ack("cmd-1", "node-1").await;
        manager.record_ack("cmd-1", "node-2").await;

        // Wait for timeout
        sleep(Duration::from_millis(150)).await;

        // Check for timeouts - should be empty since all acks received
        let timed_out = manager.check_ack_timeouts().await;
        assert_eq!(timed_out.len(), 0);
    }

    #[tokio::test]
    async fn test_unregister_ack_timeout() {
        let manager = TimeoutManager::new();

        manager
            .register_ack_timeout(
                "cmd-1".to_string(),
                vec!["node-1".to_string()],
                Duration::from_secs(30),
            )
            .await
            .unwrap();

        assert_eq!(manager.ack_timeout_count().await, 1);

        manager.unregister_ack_timeout("cmd-1").await.unwrap();
        assert_eq!(manager.ack_timeout_count().await, 0);
    }

    #[tokio::test]
    async fn test_multiple_commands_same_expiration() {
        let manager = TimeoutManager::new();

        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Two commands with same expiration time
        let cmd1 = create_test_command_with_ttl("cmd-1", now_secs - 10);
        let cmd2 = create_test_command_with_ttl("cmd-2", now_secs - 10);

        manager.register_expiration(&cmd1).await.unwrap();
        manager.register_expiration(&cmd2).await.unwrap();

        let expired = manager.process_expired().await;

        assert_eq!(expired.len(), 2);
        assert!(expired.contains(&"cmd-1".to_string()));
        assert!(expired.contains(&"cmd-2".to_string()));
    }

    #[tokio::test]
    async fn test_duplicate_ack_not_counted_twice() {
        let manager = TimeoutManager::new();

        manager
            .register_ack_timeout(
                "cmd-1".to_string(),
                vec!["node-1".to_string(), "node-2".to_string()],
                Duration::from_secs(30),
            )
            .await
            .unwrap();

        // Record same ack twice
        manager.record_ack("cmd-1", "node-1").await;
        manager.record_ack("cmd-1", "node-1").await;

        let status = manager.get_ack_status("cmd-1").await.unwrap();
        assert_eq!(status.received_acks.len(), 1); // Should only count once
    }
}
