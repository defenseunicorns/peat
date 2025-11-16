//! Core types for persistence layer

use serde::{Deserialize, Serialize};

/// Unique document identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for DocumentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for DocumentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Query builder for filtering documents
#[derive(Debug, Clone)]
pub struct Query {
    pub(crate) filters: Vec<Filter>,
    pub(crate) sort: Option<Sort>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

impl Query {
    /// Create a new empty query (matches all documents)
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            sort: None,
            limit: None,
            offset: None,
        }
    }

    /// Query all documents (no filtering)
    pub fn all() -> Self {
        Self::new()
    }

    /// Add a filter condition
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Set sort order
    pub fn sort(mut self, field: impl Into<String>, order: SortOrder) -> Self {
        self.sort = Some(Sort {
            field: field.into(),
            order,
        });
        self
    }

    /// Limit number of results
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set offset for pagination
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

impl Default for Query {
    fn default() -> Self {
        Self::new()
    }
}

/// Filter condition for queries
#[derive(Debug, Clone)]
pub enum Filter {
    /// Field equals value
    Eq(String, serde_json::Value),
    /// Field not equals value
    Ne(String, serde_json::Value),
    /// Field greater than value
    Gt(String, serde_json::Value),
    /// Field greater than or equal to value
    Gte(String, serde_json::Value),
    /// Field less than value
    Lt(String, serde_json::Value),
    /// Field less than or equal to value
    Lte(String, serde_json::Value),
    /// Field contains value (for strings)
    Contains(String, String),
    /// Field starts with value (for strings)
    StartsWith(String, String),
    /// Field is in list of values
    In(String, Vec<serde_json::Value>),
    /// Logical AND of filters
    And(Vec<Filter>),
    /// Logical OR of filters
    Or(Vec<Filter>),
}

/// Sort configuration
#[derive(Debug, Clone)]
pub struct Sort {
    pub field: String,
    pub order: SortOrder,
}

/// Sort order direction
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Document with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Document ID (if persisted)
    pub id: Option<DocumentId>,
    /// Document fields as JSON
    pub fields: serde_json::Value,
    /// Metadata (timestamps, version, etc.)
    pub metadata: DocumentMetadata,
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocumentMetadata {
    /// When document was created
    pub created_at: Option<i64>,
    /// When document was last updated
    pub updated_at: Option<i64>,
    /// Document version (for optimistic locking)
    pub version: Option<u64>,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_id_creation() {
        let id = DocumentId::new("test-id");
        assert_eq!(id.as_str(), "test-id");
    }

    #[test]
    fn test_document_id_from_string() {
        let id: DocumentId = "another-id".into();
        assert_eq!(id.as_str(), "another-id");
    }

    #[test]
    fn test_document_id_display() {
        let id = DocumentId::new("display-test");
        assert_eq!(format!("{}", id), "display-test");
    }

    #[test]
    fn test_query_builder() {
        let query = Query::new()
            .limit(10)
            .offset(5)
            .sort("created_at", SortOrder::Descending);

        assert_eq!(query.limit, Some(10));
        assert_eq!(query.offset, Some(5));
        assert!(query.sort.is_some());
    }

    #[test]
    fn test_query_all() {
        let query = Query::all();
        assert!(query.filters.is_empty());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_filter_eq() {
        let filter = Filter::Eq("status".to_string(), serde_json::json!("active"));
        match filter {
            Filter::Eq(field, _) => assert_eq!(field, "status"),
            _ => panic!("Wrong filter type"),
        }
    }

    #[test]
    fn test_document_metadata_default() {
        let metadata = DocumentMetadata::default();
        assert!(metadata.created_at.is_none());
        assert!(metadata.updated_at.is_none());
        assert!(metadata.version.is_none());
    }
}
