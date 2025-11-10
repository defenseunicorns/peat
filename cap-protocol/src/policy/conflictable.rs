//! Conflictable trait and supporting types
//!
//! Defines the interface for types that can participate in conflict resolution.

use std::collections::HashMap;

/// Generic attribute value for policy resolution
///
/// Allows policies to access type-specific data without knowing the concrete type.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    /// String value
    String(String),
    /// Signed integer value
    Int(i64),
    /// Unsigned integer value
    Uint(u64),
    /// Floating point value
    Float(f64),
    /// Boolean value
    Bool(bool),
    /// Timestamp (seconds since UNIX epoch)
    Timestamp(u64),
}

impl AttributeValue {
    /// Extract as i64, returning 0 if not an Int
    pub fn as_int(&self) -> i64 {
        match self {
            AttributeValue::Int(v) => *v,
            AttributeValue::Uint(v) => *v as i64,
            _ => 0,
        }
    }

    /// Extract as u64, returning 0 if not a Uint or Timestamp
    pub fn as_uint(&self) -> u64 {
        match self {
            AttributeValue::Uint(v) => *v,
            AttributeValue::Timestamp(v) => *v,
            AttributeValue::Int(v) => *v as u64,
            _ => 0,
        }
    }

    /// Extract as f64, returning 0.0 if not a Float
    pub fn as_float(&self) -> f64 {
        match self {
            AttributeValue::Float(v) => *v,
            AttributeValue::Int(v) => *v as f64,
            AttributeValue::Uint(v) => *v as f64,
            _ => 0.0,
        }
    }

    /// Extract as bool, returning false if not a Bool
    pub fn as_bool(&self) -> bool {
        match self {
            AttributeValue::Bool(v) => *v,
            _ => false,
        }
    }

    /// Extract as String, returning empty string if not a String
    pub fn as_string(&self) -> String {
        match self {
            AttributeValue::String(v) => v.clone(),
            _ => String::new(),
        }
    }
}

/// Result of conflict detection
#[derive(Debug, Clone)]
pub enum ConflictResult<T> {
    /// No conflict detected
    NoConflict,
    /// Conflict detected with existing items
    Conflict(Vec<T>),
}

impl<T> ConflictResult<T> {
    /// Check if this is a conflict
    pub fn is_conflict(&self) -> bool {
        matches!(self, ConflictResult::Conflict(_))
    }

    /// Get conflicting items, if any
    pub fn conflicting_items(self) -> Option<Vec<T>> {
        match self {
            ConflictResult::Conflict(items) => Some(items),
            ConflictResult::NoConflict => None,
        }
    }
}

/// Trait for types that can participate in conflict resolution
///
/// Any type implementing this trait can use the generic policy engine
/// for conflict detection and resolution.
///
/// ## Example
///
/// ```rust,ignore
/// use cap_protocol::policy::{Conflictable, AttributeValue};
/// use std::collections::HashMap;
///
/// #[derive(Clone)]
/// struct MyCommand {
///     id: String,
///     target: String,
///     priority: i32,
///     issued_at: u64,
/// }
///
/// impl Conflictable for MyCommand {
///     fn id(&self) -> String {
///         self.id.clone()
///     }
///
///     fn conflict_keys(&self) -> Vec<String> {
///         vec![self.target.clone()]
///     }
///
///     fn timestamp(&self) -> Option<u64> {
///         Some(self.issued_at)
///     }
///
///     fn attributes(&self) -> HashMap<String, AttributeValue> {
///         let mut attrs = HashMap::new();
///         attrs.insert("priority".to_string(), AttributeValue::Int(self.priority as i64));
///         attrs
///     }
/// }
/// ```
pub trait Conflictable: Clone + Send + Sync + 'static {
    /// Unique identifier for this item
    ///
    /// Used for tracking and deduplication.
    fn id(&self) -> String;

    /// Resource keys this item affects (for conflict detection)
    ///
    /// Items conflict if they share any conflict keys.
    ///
    /// ## Examples
    ///
    /// - **Commands**: `vec![target_id]` - conflict on same target
    /// - **Nodes**: `vec![format!("geo:{}:{}", lat, lon)]` - conflict on same location
    /// - **Capabilities**: `vec![format!("resource:{}", resource_type)]` - conflict on same resource
    fn conflict_keys(&self) -> Vec<String>;

    /// Timestamp for recency-based resolution (optional)
    ///
    /// Used by policies like `LastWriteWinsPolicy`.
    /// Returns seconds since UNIX epoch.
    fn timestamp(&self) -> Option<u64>;

    /// Custom attributes for policy resolution
    ///
    /// Allows policies to access type-specific data without knowing the type.
    /// Common attributes:
    /// - "priority" - for priority-based resolution
    /// - "authority_level" - for authority-based resolution
    /// - "confidence" - for confidence-based resolution
    fn attributes(&self) -> HashMap<String, AttributeValue>;
}
