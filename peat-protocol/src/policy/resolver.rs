//! Generic conflict resolver implementation
//!
//! Provides conflict detection and resolution for any type implementing Conflictable.

use crate::error::Result;
use crate::policy::conflictable::{ConflictResult, Conflictable};
use crate::policy::policies::ResolutionPolicy;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Generic conflict resolver for any `Conflictable` type
///
/// Maintains an index of active items by conflict key for efficient detection.
/// Works with any type implementing the `Conflictable` trait.
///
/// ## Example
///
/// ```rust,ignore
/// use peat_protocol::policy::{GenericConflictResolver, LastWriteWinsPolicy, Conflictable, ConflictResult};
///
/// // Create a resolver for your type
/// let resolver = GenericConflictResolver::<MyType>::new();
///
/// // Check for conflicts
/// let result = resolver.check_conflict(&my_item).await;
///
/// // Resolve if needed
/// if let ConflictResult::Conflict(existing) = result {
///     let policy = LastWriteWinsPolicy;
///     let winner = resolver.resolve(vec![existing[0].clone(), my_item], &policy)?;
/// }
/// ```
pub struct GenericConflictResolver<T: Conflictable> {
    /// Active items indexed by conflict key
    /// Key: conflict_key, Value: list of items with that key
    active_items: Arc<RwLock<HashMap<String, Vec<T>>>>,
    _phantom: PhantomData<T>,
}

impl<T: Conflictable> GenericConflictResolver<T> {
    /// Create a new generic conflict resolver
    pub fn new() -> Self {
        Self {
            active_items: Arc::new(RwLock::new(HashMap::new())),
            _phantom: PhantomData,
        }
    }

    /// Check if a new item conflicts with existing items
    ///
    /// Returns `ConflictResult::Conflict` if there are existing items
    /// with overlapping conflict keys.
    pub async fn check_conflict(&self, item: &T) -> ConflictResult<T> {
        let keys = item.conflict_keys();
        let items = self.active_items.read().await;

        let mut conflicting = Vec::new();

        for key in keys {
            if let Some(existing) = items.get(&key) {
                // Avoid duplicates
                for existing_item in existing {
                    if !conflicting.iter().any(|c: &T| c.id() == existing_item.id()) {
                        conflicting.push(existing_item.clone());
                    }
                }
            }
        }

        if conflicting.is_empty() {
            ConflictResult::NoConflict
        } else {
            ConflictResult::Conflict(conflicting)
        }
    }

    /// Resolve conflict using the specified policy
    ///
    /// Takes a list of conflicting items and returns the "winning" item
    /// according to the policy's logic.
    pub fn resolve(&self, items: Vec<T>, policy: &dyn ResolutionPolicy<T>) -> Result<T> {
        tracing::debug!(
            "Resolving conflict between {} items using policy: {}",
            items.len(),
            policy.name()
        );

        policy.resolve(items)
    }

    /// Register an item as active (after conflict resolution)
    ///
    /// Adds the item to the conflict index for future conflict checks.
    pub async fn register(&self, item: &T) -> Result<()> {
        let keys = item.conflict_keys();
        let mut items = self.active_items.write().await;

        for key in keys {
            items.entry(key).or_default().push(item.clone());
        }

        Ok(())
    }

    /// Unregister an item from active tracking
    ///
    /// Removes the item from all conflict key indices.
    /// Called when an item completes, expires, or is cancelled.
    pub async fn unregister(&self, item_id: &str) -> Result<()> {
        let mut items = self.active_items.write().await;

        // Remove from all key lists
        for (_, item_list) in items.iter_mut() {
            item_list.retain(|item| item.id() != item_id);
        }

        // Clean up empty keys
        items.retain(|_, item_list| !item_list.is_empty());

        Ok(())
    }

    /// Get all active items
    pub async fn get_all_active(&self) -> Vec<T> {
        let items = self.active_items.read().await;
        let mut all_items = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for item_list in items.values() {
            for item in item_list {
                if seen_ids.insert(item.id()) {
                    all_items.push(item.clone());
                }
            }
        }

        all_items
    }

    /// Get count of active items
    pub async fn active_count(&self) -> usize {
        self.get_all_active().await.len()
    }

    /// Get active items by conflict key
    pub async fn get_by_key(&self, key: &str) -> Vec<T> {
        self.active_items
            .read()
            .await
            .get(key)
            .cloned()
            .unwrap_or_default()
    }
}

impl<T: Conflictable> Default for GenericConflictResolver<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::conflictable::AttributeValue;
    use crate::policy::policies::{HighestAttributeWinsPolicy, LastWriteWinsPolicy};
    use std::collections::HashMap;

    #[derive(Clone, Debug, PartialEq)]
    struct TestItem {
        id: String,
        resource: String,
        timestamp: u64,
        priority: i64,
    }

    impl Conflictable for TestItem {
        fn id(&self) -> String {
            self.id.clone()
        }

        fn conflict_keys(&self) -> Vec<String> {
            vec![self.resource.clone()]
        }

        fn timestamp(&self) -> Option<u64> {
            Some(self.timestamp)
        }

        fn attributes(&self) -> HashMap<String, AttributeValue> {
            let mut attrs = HashMap::new();
            attrs.insert("priority".to_string(), AttributeValue::Int(self.priority));
            attrs
        }
    }

    #[tokio::test]
    async fn test_no_conflict_different_resources() {
        let resolver = GenericConflictResolver::<TestItem>::new();

        let item1 = TestItem {
            id: "item-1".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 1000,
            priority: 1,
        };

        resolver.register(&item1).await.unwrap();

        let item2 = TestItem {
            id: "item-2".to_string(),
            resource: "resource-b".to_string(),
            timestamp: 1001,
            priority: 2,
        };

        let result = resolver.check_conflict(&item2).await;
        assert!(!result.is_conflict());
    }

    #[tokio::test]
    async fn test_conflict_same_resource() {
        let resolver = GenericConflictResolver::<TestItem>::new();

        let item1 = TestItem {
            id: "item-1".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 1000,
            priority: 1,
        };

        resolver.register(&item1).await.unwrap();

        let item2 = TestItem {
            id: "item-2".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 1001,
            priority: 2,
        };

        let result = resolver.check_conflict(&item2).await;
        assert!(result.is_conflict());

        if let ConflictResult::Conflict(items) = result {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].id, "item-1");
        }
    }

    #[tokio::test]
    async fn test_resolve_last_write_wins() {
        let resolver = GenericConflictResolver::<TestItem>::new();

        let item1 = TestItem {
            id: "item-1".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 1000,
            priority: 1,
        };

        let item2 = TestItem {
            id: "item-2".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 2000,
            priority: 2,
        };

        let policy = LastWriteWinsPolicy;
        let winner = resolver.resolve(vec![item1, item2], &policy).unwrap();

        assert_eq!(winner.id, "item-2"); // Most recent
    }

    #[tokio::test]
    async fn test_resolve_highest_priority() {
        let resolver = GenericConflictResolver::<TestItem>::new();

        let item1 = TestItem {
            id: "item-1".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 2000,
            priority: 1,
        };

        let item2 = TestItem {
            id: "item-2".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 1000,
            priority: 5,
        };

        let policy = HighestAttributeWinsPolicy::new("priority");
        let winner = resolver.resolve(vec![item1, item2], &policy).unwrap();

        assert_eq!(winner.id, "item-2"); // Highest priority
    }

    #[tokio::test]
    async fn test_unregister() {
        let resolver = GenericConflictResolver::<TestItem>::new();

        let item1 = TestItem {
            id: "item-1".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 1000,
            priority: 1,
        };

        resolver.register(&item1).await.unwrap();
        assert_eq!(resolver.active_count().await, 1);

        resolver.unregister("item-1").await.unwrap();
        assert_eq!(resolver.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_get_by_key() {
        let resolver = GenericConflictResolver::<TestItem>::new();

        let item1 = TestItem {
            id: "item-1".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 1000,
            priority: 1,
        };

        let item2 = TestItem {
            id: "item-2".to_string(),
            resource: "resource-a".to_string(),
            timestamp: 1001,
            priority: 2,
        };

        resolver.register(&item1).await.unwrap();
        resolver.register(&item2).await.unwrap();

        let items = resolver.get_by_key("resource-a").await;
        assert_eq!(items.len(), 2);
    }
}
