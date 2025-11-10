//! Resolution policies for conflict resolution
//!
//! Defines the trait and common implementations for conflict resolution strategies.

use crate::error::Result;
use crate::policy::conflictable::{AttributeValue, Conflictable};

/// Trait for conflict resolution policies
///
/// Implement this trait to define custom conflict resolution strategies.
///
/// ## Example
///
/// ```rust,no_run
/// use cap_protocol::policy::{ResolutionPolicy, Conflictable};
/// use cap_protocol::error::Result;
///
/// struct MyCustomPolicy;
///
/// impl<T: Conflictable> ResolutionPolicy<T> for MyCustomPolicy {
///     fn resolve(&self, mut items: Vec<T>) -> Result<T> {
///         // Custom logic here
///         Ok(items.into_iter().next().unwrap())
///     }
///
///     fn name(&self) -> &str {
///         "MY_CUSTOM_POLICY"
///     }
/// }
/// ```
pub trait ResolutionPolicy<T: Conflictable>: Send + Sync {
    /// Resolve conflict between multiple items
    ///
    /// Takes a list of conflicting items and returns the "winning" item
    /// according to the policy's logic.
    ///
    /// # Errors
    ///
    /// Returns an error if the policy cannot resolve the conflict
    /// (e.g., all items are equal, required attributes missing).
    fn resolve(&self, items: Vec<T>) -> Result<T>;

    /// Policy name for logging and debugging
    fn name(&self) -> &str;
}

/// Policy: Most recent item wins (based on timestamp)
///
/// Uses the `timestamp()` method from Conflictable.
/// Items without timestamps are treated as oldest.
pub struct LastWriteWinsPolicy;

impl<T: Conflictable> ResolutionPolicy<T> for LastWriteWinsPolicy {
    fn resolve(&self, mut items: Vec<T>) -> Result<T> {
        if items.is_empty() {
            return Err(crate::Error::InvalidInput(
                "Cannot resolve empty item list".to_string(),
            ));
        }

        if items.len() == 1 {
            return Ok(items.into_iter().next().unwrap());
        }

        items.sort_by(|a, b| {
            let a_time = a.timestamp().unwrap_or(0);
            let b_time = b.timestamp().unwrap_or(0);
            b_time.cmp(&a_time) // Most recent first
        });

        Ok(items.into_iter().next().unwrap())
    }

    fn name(&self) -> &str {
        "LAST_WRITE_WINS"
    }
}

/// Policy: Item with highest value for specified attribute wins
///
/// Supports Int, Uint, and Float attribute types.
/// Items without the attribute are treated as having value 0.
///
/// ## Example
///
/// ```rust,no_run
/// use cap_protocol::policy::HighestAttributeWinsPolicy;
///
/// // Resolve based on "priority" attribute
/// let policy = HighestAttributeWinsPolicy::new("priority");
///
/// // Resolve based on "confidence" attribute
/// let policy = HighestAttributeWinsPolicy::new("confidence");
/// ```
pub struct HighestAttributeWinsPolicy {
    attribute_name: String,
}

impl HighestAttributeWinsPolicy {
    /// Create a new policy that selects based on highest attribute value
    pub fn new(attribute_name: impl Into<String>) -> Self {
        Self {
            attribute_name: attribute_name.into(),
        }
    }
}

impl<T: Conflictable> ResolutionPolicy<T> for HighestAttributeWinsPolicy {
    fn resolve(&self, mut items: Vec<T>) -> Result<T> {
        if items.is_empty() {
            return Err(crate::Error::InvalidInput(
                "Cannot resolve empty item list".to_string(),
            ));
        }

        if items.len() == 1 {
            return Ok(items.into_iter().next().unwrap());
        }

        items.sort_by(|a, b| {
            let a_attrs = a.attributes();
            let b_attrs = b.attributes();

            let a_val = a_attrs.get(&self.attribute_name);
            let b_val = b_attrs.get(&self.attribute_name);

            match (a_val, b_val) {
                (Some(AttributeValue::Int(a)), Some(AttributeValue::Int(b))) => b.cmp(a),
                (Some(AttributeValue::Uint(a)), Some(AttributeValue::Uint(b))) => b.cmp(a),
                (Some(AttributeValue::Float(a)), Some(AttributeValue::Float(b))) => {
                    b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
                }
                (Some(a_val), Some(b_val)) => {
                    // Try to coerce to comparable types
                    let a_float = a_val.as_float();
                    let b_float = b_val.as_float();
                    b_float
                        .partial_cmp(&a_float)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                (Some(_), None) => std::cmp::Ordering::Less, // a wins (has attribute)
                (None, Some(_)) => std::cmp::Ordering::Greater, // b wins (has attribute)
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        Ok(items.into_iter().next().unwrap())
    }

    fn name(&self) -> &str {
        &self.attribute_name
    }
}

/// Policy: Reject all conflicts (return error)
///
/// Use this when conflicts are not allowed and should be treated as errors.
pub struct RejectConflictPolicy;

impl<T: Conflictable> ResolutionPolicy<T> for RejectConflictPolicy {
    fn resolve(&self, items: Vec<T>) -> Result<T> {
        if items.len() > 1 {
            return Err(crate::Error::ConflictDetected(format!(
                "Conflict detected between {} items (policy: REJECT)",
                items.len()
            )));
        }

        items
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::InvalidInput("Empty item list".to_string()))
    }

    fn name(&self) -> &str {
        "REJECT_CONFLICT"
    }
}

/// Policy: Item with lowest value for specified attribute wins
///
/// Opposite of `HighestAttributeWinsPolicy`.
/// Useful for selecting items with lowest cost, earliest deadline, etc.
#[allow(dead_code)]
pub struct LowestAttributeWinsPolicy {
    attribute_name: String,
}

#[allow(dead_code)]
impl LowestAttributeWinsPolicy {
    /// Create a new policy that selects based on lowest attribute value
    pub fn new(attribute_name: impl Into<String>) -> Self {
        Self {
            attribute_name: attribute_name.into(),
        }
    }
}

impl<T: Conflictable> ResolutionPolicy<T> for LowestAttributeWinsPolicy {
    fn resolve(&self, mut items: Vec<T>) -> Result<T> {
        if items.is_empty() {
            return Err(crate::Error::InvalidInput(
                "Cannot resolve empty item list".to_string(),
            ));
        }

        if items.len() == 1 {
            return Ok(items.into_iter().next().unwrap());
        }

        items.sort_by(|a, b| {
            let a_attrs = a.attributes();
            let b_attrs = b.attributes();

            let a_val = a_attrs.get(&self.attribute_name);
            let b_val = b_attrs.get(&self.attribute_name);

            match (a_val, b_val) {
                (Some(AttributeValue::Int(a)), Some(AttributeValue::Int(b))) => a.cmp(b),
                (Some(AttributeValue::Uint(a)), Some(AttributeValue::Uint(b))) => a.cmp(b),
                (Some(AttributeValue::Float(a)), Some(AttributeValue::Float(b))) => {
                    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                }
                (Some(a_val), Some(b_val)) => {
                    let a_float = a_val.as_float();
                    let b_float = b_val.as_float();
                    a_float
                        .partial_cmp(&b_float)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                (Some(_), None) => std::cmp::Ordering::Less, // a wins (has attribute)
                (None, Some(_)) => std::cmp::Ordering::Greater, // b wins (has attribute)
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        Ok(items.into_iter().next().unwrap())
    }

    fn name(&self) -> &str {
        &self.attribute_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Test type for policy tests
    #[derive(Clone)]
    struct TestItem {
        id: String,
        timestamp: u64,
        priority: i64,
        confidence: f64,
    }

    impl Conflictable for TestItem {
        fn id(&self) -> String {
            self.id.clone()
        }

        fn conflict_keys(&self) -> Vec<String> {
            vec!["test-key".to_string()]
        }

        fn timestamp(&self) -> Option<u64> {
            Some(self.timestamp)
        }

        fn attributes(&self) -> HashMap<String, AttributeValue> {
            let mut attrs = HashMap::new();
            attrs.insert("priority".to_string(), AttributeValue::Int(self.priority));
            attrs.insert(
                "confidence".to_string(),
                AttributeValue::Float(self.confidence),
            );
            attrs
        }
    }

    #[test]
    fn test_last_write_wins() {
        let policy = LastWriteWinsPolicy;

        let items = vec![
            TestItem {
                id: "item-1".to_string(),
                timestamp: 1000,
                priority: 1,
                confidence: 0.5,
            },
            TestItem {
                id: "item-2".to_string(),
                timestamp: 2000,
                priority: 2,
                confidence: 0.7,
            },
            TestItem {
                id: "item-3".to_string(),
                timestamp: 1500,
                priority: 3,
                confidence: 0.9,
            },
        ];

        let winner = policy.resolve(items).unwrap();
        assert_eq!(winner.id, "item-2"); // Most recent (2000)
    }

    #[test]
    fn test_highest_attribute_wins_int() {
        let policy = HighestAttributeWinsPolicy::new("priority");

        let items = vec![
            TestItem {
                id: "item-1".to_string(),
                timestamp: 1000,
                priority: 1,
                confidence: 0.5,
            },
            TestItem {
                id: "item-2".to_string(),
                timestamp: 2000,
                priority: 5,
                confidence: 0.7,
            },
            TestItem {
                id: "item-3".to_string(),
                timestamp: 1500,
                priority: 3,
                confidence: 0.9,
            },
        ];

        let winner = policy.resolve(items).unwrap();
        assert_eq!(winner.id, "item-2"); // Highest priority (5)
    }

    #[test]
    fn test_highest_attribute_wins_float() {
        let policy = HighestAttributeWinsPolicy::new("confidence");

        let items = vec![
            TestItem {
                id: "item-1".to_string(),
                timestamp: 1000,
                priority: 1,
                confidence: 0.5,
            },
            TestItem {
                id: "item-2".to_string(),
                timestamp: 2000,
                priority: 5,
                confidence: 0.7,
            },
            TestItem {
                id: "item-3".to_string(),
                timestamp: 1500,
                priority: 3,
                confidence: 0.95,
            },
        ];

        let winner = policy.resolve(items).unwrap();
        assert_eq!(winner.id, "item-3"); // Highest confidence (0.95)
    }

    #[test]
    fn test_reject_conflict_policy() {
        let policy = RejectConflictPolicy;

        let items = vec![
            TestItem {
                id: "item-1".to_string(),
                timestamp: 1000,
                priority: 1,
                confidence: 0.5,
            },
            TestItem {
                id: "item-2".to_string(),
                timestamp: 2000,
                priority: 5,
                confidence: 0.7,
            },
        ];

        let result = policy.resolve(items);
        assert!(result.is_err());
        assert!(matches!(result, Err(crate::Error::ConflictDetected(_))));
    }

    #[test]
    fn test_lowest_attribute_wins() {
        let policy = LowestAttributeWinsPolicy::new("priority");

        let items = vec![
            TestItem {
                id: "item-1".to_string(),
                timestamp: 1000,
                priority: 10,
                confidence: 0.5,
            },
            TestItem {
                id: "item-2".to_string(),
                timestamp: 2000,
                priority: 2,
                confidence: 0.7,
            },
            TestItem {
                id: "item-3".to_string(),
                timestamp: 1500,
                priority: 5,
                confidence: 0.9,
            },
        ];

        let winner = policy.resolve(items).unwrap();
        assert_eq!(winner.id, "item-2"); // Lowest priority (2)
    }
}
