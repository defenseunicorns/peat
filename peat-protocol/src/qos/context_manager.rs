//! Context manager for atomic mission context switching (ADR-019)
//!
//! This module provides thread-safe context management with listener support
//! for reactive components that need to respond to context changes.
//!
//! # Example
//!
//! ```
//! use peat_protocol::qos::{ContextManager, MissionContext, DataType, QoSClass};
//!
//! let manager = ContextManager::new();
//!
//! // Set context
//! manager.set_context(MissionContext::Execution);
//! assert_eq!(manager.get_context(), MissionContext::Execution);
//!
//! // Get effective QoS class for a data type in the current context
//! let class = manager.effective_class(&DataType::TargetImage);
//! assert_eq!(class, QoSClass::Critical); // Elevated in execution context
//! ```

use super::classification::DataType;
use super::context::{ContextProfile, MissionContext, QoSClassAdjustment};
use super::{QoSClass, QoSPolicy};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::RwLock;

/// Listener trait for context change notifications
pub trait ContextChangeListener: Send + Sync {
    /// Called when the mission context changes
    ///
    /// The listener receives both the old and new context, allowing
    /// it to react appropriately to the transition.
    fn on_context_change(&self, old: MissionContext, new: MissionContext);
}

/// A simple listener that stores context changes for testing/debugging
#[derive(Debug, Default)]
pub struct ContextChangeLog {
    changes: RwLock<Vec<(MissionContext, MissionContext)>>,
}

impl ContextChangeLog {
    /// Create a new empty change log
    pub fn new() -> Self {
        Self {
            changes: RwLock::new(Vec::new()),
        }
    }

    /// Get all recorded context changes
    pub fn changes(&self) -> Vec<(MissionContext, MissionContext)> {
        self.changes.read().unwrap().clone()
    }

    /// Get the number of context changes
    pub fn change_count(&self) -> usize {
        self.changes.read().unwrap().len()
    }

    /// Clear all recorded changes
    pub fn clear(&self) {
        self.changes.write().unwrap().clear();
    }
}

impl ContextChangeListener for ContextChangeLog {
    fn on_context_change(&self, old: MissionContext, new: MissionContext) {
        self.changes.write().unwrap().push((old, new));
    }
}

/// Manager for mission context with atomic switching and listener support
///
/// The ContextManager provides thread-safe access to the current mission
/// context and notifies registered listeners when the context changes.
pub struct ContextManager {
    /// Current mission context (stored as u8 for atomic access)
    current_context: AtomicU8,

    /// Custom profiles per context (overrides defaults)
    custom_profiles: RwLock<HashMap<MissionContext, ContextProfile>>,

    /// Registered listeners for context changes
    listeners: RwLock<Vec<Arc<dyn ContextChangeListener>>>,
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextManager {
    /// Create a new context manager with default (Standby) context
    pub fn new() -> Self {
        Self {
            current_context: AtomicU8::new(MissionContext::Standby as u8),
            custom_profiles: RwLock::new(HashMap::new()),
            listeners: RwLock::new(Vec::new()),
        }
    }

    /// Create a context manager with an initial context
    pub fn with_context(context: MissionContext) -> Self {
        Self {
            current_context: AtomicU8::new(context as u8),
            custom_profiles: RwLock::new(HashMap::new()),
            listeners: RwLock::new(Vec::new()),
        }
    }

    /// Get the current mission context
    pub fn get_context(&self) -> MissionContext {
        u8_to_context(self.current_context.load(Ordering::SeqCst))
    }

    /// Set the mission context atomically
    ///
    /// This method atomically updates the context and notifies all
    /// registered listeners of the change.
    pub fn set_context(&self, new_context: MissionContext) {
        let old_val = self
            .current_context
            .swap(new_context as u8, Ordering::SeqCst);
        let old_context = u8_to_context(old_val);

        // Only notify if actually changed
        if old_context != new_context {
            self.notify_listeners(old_context, new_context);
        }
    }

    /// Compare and swap the context atomically
    ///
    /// Only sets the new context if the current context matches `expected`.
    /// Returns true if the swap was successful.
    pub fn compare_and_swap(&self, expected: MissionContext, new_context: MissionContext) -> bool {
        let result = self.current_context.compare_exchange(
            expected as u8,
            new_context as u8,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );

        if result.is_ok() && expected != new_context {
            self.notify_listeners(expected, new_context);
            true
        } else {
            result.is_ok()
        }
    }

    /// Subscribe a listener for context changes
    pub fn subscribe(&self, listener: Arc<dyn ContextChangeListener>) {
        self.listeners.write().unwrap().push(listener);
    }

    /// Unsubscribe all listeners
    pub fn clear_listeners(&self) {
        self.listeners.write().unwrap().clear();
    }

    /// Get the number of registered listeners
    pub fn listener_count(&self) -> usize {
        self.listeners.read().unwrap().len()
    }

    /// Set a custom profile for a specific context
    ///
    /// This overrides the default profile for the given context.
    pub fn set_custom_profile(&self, context: MissionContext, profile: ContextProfile) {
        self.custom_profiles
            .write()
            .unwrap()
            .insert(context, profile);
    }

    /// Remove a custom profile for a context, reverting to default
    pub fn clear_custom_profile(&self, context: MissionContext) {
        self.custom_profiles.write().unwrap().remove(&context);
    }

    /// Get the profile for the current context
    pub fn current_profile(&self) -> ContextProfile {
        self.profile_for(self.get_context())
    }

    /// Get the profile for a specific context
    pub fn profile_for(&self, context: MissionContext) -> ContextProfile {
        self.custom_profiles
            .read()
            .unwrap()
            .get(&context)
            .cloned()
            .unwrap_or_else(|| context.profile())
    }

    /// Get the adjustment for a data type in the current context
    pub fn get_adjustment(&self, data_type: &DataType) -> QoSClassAdjustment {
        self.current_profile().get_adjustment(data_type)
    }

    /// Apply the current context profile to a base QoS policy
    ///
    /// Returns the adjusted policy for the given data type.
    pub fn adjust_policy(&self, base: &QoSPolicy, data_type: &DataType) -> QoSPolicy {
        self.current_profile().apply_to_policy(base, data_type)
    }

    /// Get the effective QoS class for a data type in the current context
    ///
    /// This applies the context adjustment to the data type's default class.
    pub fn effective_class(&self, data_type: &DataType) -> QoSClass {
        let base_class = data_type.default_class();
        let adjustment = self.get_adjustment(data_type);
        adjustment.apply(base_class)
    }

    /// Check if bulk sync should be enabled in the current context
    pub fn enables_bulk_sync(&self) -> bool {
        self.get_context().enables_bulk_sync()
    }

    /// Notify all listeners of a context change
    fn notify_listeners(&self, old: MissionContext, new: MissionContext) {
        let listeners = self.listeners.read().unwrap();
        for listener in listeners.iter() {
            listener.on_context_change(old, new);
        }
    }
}

/// Convert u8 to MissionContext
fn u8_to_context(val: u8) -> MissionContext {
    match val {
        0 => MissionContext::Ingress,
        1 => MissionContext::Execution,
        2 => MissionContext::Egress,
        3 => MissionContext::Emergency,
        _ => MissionContext::Standby,
    }
}

impl std::fmt::Debug for ContextManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextManager")
            .field("current_context", &self.get_context())
            .field("listener_count", &self.listener_count())
            .field(
                "custom_profiles",
                &self
                    .custom_profiles
                    .read()
                    .unwrap()
                    .keys()
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_manager_default() {
        let manager = ContextManager::new();
        assert_eq!(manager.get_context(), MissionContext::Standby);
    }

    #[test]
    fn test_context_manager_with_context() {
        let manager = ContextManager::with_context(MissionContext::Execution);
        assert_eq!(manager.get_context(), MissionContext::Execution);
    }

    #[test]
    fn test_set_context() {
        let manager = ContextManager::new();

        manager.set_context(MissionContext::Ingress);
        assert_eq!(manager.get_context(), MissionContext::Ingress);

        manager.set_context(MissionContext::Emergency);
        assert_eq!(manager.get_context(), MissionContext::Emergency);
    }

    #[test]
    fn test_compare_and_swap_success() {
        let manager = ContextManager::with_context(MissionContext::Standby);

        let success = manager.compare_and_swap(MissionContext::Standby, MissionContext::Execution);

        assert!(success);
        assert_eq!(manager.get_context(), MissionContext::Execution);
    }

    #[test]
    fn test_compare_and_swap_failure() {
        let manager = ContextManager::with_context(MissionContext::Standby);

        // Try to swap from wrong expected value
        let success = manager.compare_and_swap(MissionContext::Ingress, MissionContext::Execution);

        assert!(!success);
        assert_eq!(manager.get_context(), MissionContext::Standby);
    }

    #[test]
    fn test_listener_notification() {
        let manager = ContextManager::new();
        let log = Arc::new(ContextChangeLog::new());

        manager.subscribe(log.clone());

        manager.set_context(MissionContext::Execution);
        manager.set_context(MissionContext::Egress);

        let changes = log.changes();
        assert_eq!(changes.len(), 2);
        assert_eq!(
            changes[0],
            (MissionContext::Standby, MissionContext::Execution)
        );
        assert_eq!(
            changes[1],
            (MissionContext::Execution, MissionContext::Egress)
        );
    }

    #[test]
    fn test_no_notification_same_context() {
        let manager = ContextManager::with_context(MissionContext::Standby);
        let log = Arc::new(ContextChangeLog::new());

        manager.subscribe(log.clone());

        // Set to same context
        manager.set_context(MissionContext::Standby);

        // Should not have any changes
        assert_eq!(log.change_count(), 0);
    }

    #[test]
    fn test_custom_profile() {
        let manager = ContextManager::new();

        // Create custom profile with additional elevation
        let mut custom = ContextProfile::execution();
        custom.set_adjustment(DataType::HealthStatus, QoSClassAdjustment::Elevate(2));

        manager.set_custom_profile(MissionContext::Execution, custom);
        manager.set_context(MissionContext::Execution);

        // Should use custom adjustment
        let adj = manager.get_adjustment(&DataType::HealthStatus);
        assert_eq!(adj, QoSClassAdjustment::Elevate(2));
    }

    #[test]
    fn test_clear_custom_profile() {
        let manager = ContextManager::new();

        let mut custom = ContextProfile::execution();
        custom.set_adjustment(DataType::HealthStatus, QoSClassAdjustment::Elevate(2));

        manager.set_custom_profile(MissionContext::Execution, custom);
        manager.clear_custom_profile(MissionContext::Execution);
        manager.set_context(MissionContext::Execution);

        // Should use default (no adjustment for HealthStatus in execution)
        let adj = manager.get_adjustment(&DataType::HealthStatus);
        assert_eq!(adj, QoSClassAdjustment::NoChange);
    }

    #[test]
    fn test_effective_class() {
        let manager = ContextManager::with_context(MissionContext::Execution);

        // Target image: P2 → P1 (elevated in execution)
        let class = manager.effective_class(&DataType::TargetImage);
        assert_eq!(class, QoSClass::Critical);

        // Contact report: P1 → P1 (unchanged)
        let class = manager.effective_class(&DataType::ContactReport);
        assert_eq!(class, QoSClass::Critical);

        // Debug log: P5 → P5 (unchanged)
        let class = manager.effective_class(&DataType::DebugLog);
        assert_eq!(class, QoSClass::Bulk);
    }

    #[test]
    fn test_adjust_policy() {
        let manager = ContextManager::with_context(MissionContext::Emergency);
        let base = DataType::HealthStatus.default_policy();

        let adjusted = manager.adjust_policy(&base, &DataType::HealthStatus);

        // Should be elevated to critical in emergency
        assert_eq!(adjusted.base_class, QoSClass::Critical);
        // Latency should be tighter
        assert!(adjusted.max_latency_ms < base.max_latency_ms);
    }

    #[test]
    fn test_enables_bulk_sync() {
        let manager = ContextManager::new();

        // Standby enables bulk sync
        assert!(manager.enables_bulk_sync());

        manager.set_context(MissionContext::Emergency);
        assert!(!manager.enables_bulk_sync());

        manager.set_context(MissionContext::Standby);
        assert!(manager.enables_bulk_sync());
    }

    #[test]
    fn test_clear_listeners() {
        let manager = ContextManager::new();
        let log = Arc::new(ContextChangeLog::new());

        manager.subscribe(log.clone());
        assert_eq!(manager.listener_count(), 1);

        manager.clear_listeners();
        assert_eq!(manager.listener_count(), 0);

        // Changes should not be logged after clearing
        manager.set_context(MissionContext::Execution);
        assert_eq!(log.change_count(), 0);
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;

        let manager = Arc::new(ContextManager::new());
        let log = Arc::new(ContextChangeLog::new());

        manager.subscribe(log.clone());

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let manager = manager.clone();
                thread::spawn(move || {
                    let context = match i % 4 {
                        0 => MissionContext::Ingress,
                        1 => MissionContext::Execution,
                        2 => MissionContext::Egress,
                        _ => MissionContext::Emergency,
                    };
                    for _ in 0..10 {
                        manager.set_context(context);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have completed without panics
        // Final context will be one of the 4 options
        let final_context = manager.get_context();
        assert!(matches!(
            final_context,
            MissionContext::Ingress
                | MissionContext::Execution
                | MissionContext::Egress
                | MissionContext::Emergency
        ));
    }

    #[test]
    fn test_debug_impl() {
        let manager = ContextManager::with_context(MissionContext::Execution);
        let debug_str = format!("{:?}", manager);
        assert!(debug_str.contains("Execution"));
    }
}
