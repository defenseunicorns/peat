//! Query engine for Automerge documents
//!
//! Provides a fluent builder API for querying Automerge documents stored in RocksDB
//! with support for field-based filtering, sorting, and pagination.
//!
//! # Example
//!
//! ```ignore
//! use hive_protocol::storage::{Query, SortOrder, Value};
//!
//! let results = Query::new(store.clone(), "beacons")
//!     .where_eq("operational", Value::Bool(true))
//!     .where_gt("fuel_percent", Value::Int(20))
//!     .order_by("timestamp", SortOrder::Desc)
//!     .limit(10)
//!     .execute()?;
//! ```

#[cfg(feature = "automerge-backend")]
use crate::storage::AutomergeStore;
#[cfg(feature = "automerge-backend")]
use anyhow::Result;
#[cfg(feature = "automerge-backend")]
use automerge::{Automerge, ReadDoc};
#[cfg(feature = "automerge-backend")]
use std::collections::HashSet;
#[cfg(feature = "automerge-backend")]
use std::sync::Arc;

/// Sort order for query results
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    /// Ascending order (smallest first)
    #[default]
    Asc,
    /// Descending order (largest first)
    Desc,
}

/// Query value types for comparisons
///
/// Maps to Automerge scalar types for type-safe field comparisons.
#[cfg(feature = "automerge-backend")]
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Signed integer
    Int(i64),
    /// Unsigned integer
    Uint(u64),
    /// Floating point
    Float(f64),
    /// String value
    String(String),
    /// Unix timestamp (seconds since epoch)
    Timestamp(i64),
}

#[cfg(feature = "automerge-backend")]
impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        match (self, other) {
            (Value::Null, Value::Null) => Some(Ordering::Equal),
            (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),
            (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
            (Value::Uint(a), Value::Uint(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
            (Value::String(a), Value::String(b)) => a.partial_cmp(b),
            (Value::Timestamp(a), Value::Timestamp(b)) => a.partial_cmp(b),
            // Cross-type numeric comparisons
            (Value::Int(a), Value::Uint(b)) => (*a as f64).partial_cmp(&(*b as f64)),
            (Value::Uint(a), Value::Int(b)) => (*a as f64).partial_cmp(&(*b as f64)),
            (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
            (Value::Uint(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Uint(b)) => a.partial_cmp(&(*b as f64)),
            // Incompatible types
            _ => None,
        }
    }
}

/// Predicate type for document filtering
#[cfg(feature = "automerge-backend")]
type Predicate = Box<dyn Fn(&Automerge) -> bool + Send + Sync>;

/// Query builder for Automerge documents
///
/// Provides a fluent API for building queries with filtering, sorting, and pagination.
#[cfg(feature = "automerge-backend")]
pub struct Query {
    store: Arc<AutomergeStore>,
    collection_name: String,
    predicates: Vec<Predicate>,
    sort_field: Option<(String, SortOrder)>,
    limit: Option<usize>,
    offset: usize,
    doc_id_filter: Option<HashSet<String>>,
}

#[cfg(feature = "automerge-backend")]
impl Query {
    /// Create a new query for a collection
    ///
    /// # Arguments
    ///
    /// * `store` - AutomergeStore containing the documents
    /// * `collection_name` - Name of the collection to query
    pub fn new(store: Arc<AutomergeStore>, collection_name: &str) -> Self {
        Self {
            store,
            collection_name: collection_name.to_string(),
            predicates: Vec::new(),
            sort_field: None,
            limit: None,
            offset: 0,
            doc_id_filter: None,
        }
    }

    /// Filter where field equals value
    ///
    /// # Example
    ///
    /// ```ignore
    /// query.where_eq("operational", Value::Bool(true))
    /// ```
    pub fn where_eq(mut self, field: &str, value: Value) -> Self {
        let field = field.to_string();
        self.predicates.push(Box::new(move |doc| {
            extract_field(doc, &field)
                .map(|v| v == value)
                .unwrap_or(false)
        }));
        self
    }

    /// Filter where field is greater than value
    pub fn where_gt(mut self, field: &str, value: Value) -> Self {
        let field = field.to_string();
        self.predicates.push(Box::new(move |doc| {
            extract_field(doc, &field)
                .and_then(|v| v.partial_cmp(&value))
                .map(|ord| ord == std::cmp::Ordering::Greater)
                .unwrap_or(false)
        }));
        self
    }

    /// Filter where field is less than value
    pub fn where_lt(mut self, field: &str, value: Value) -> Self {
        let field = field.to_string();
        self.predicates.push(Box::new(move |doc| {
            extract_field(doc, &field)
                .and_then(|v| v.partial_cmp(&value))
                .map(|ord| ord == std::cmp::Ordering::Less)
                .unwrap_or(false)
        }));
        self
    }

    /// Filter where field is greater than or equal to value
    pub fn where_gte(mut self, field: &str, value: Value) -> Self {
        let field = field.to_string();
        self.predicates.push(Box::new(move |doc| {
            extract_field(doc, &field)
                .and_then(|v| v.partial_cmp(&value))
                .map(|ord| ord != std::cmp::Ordering::Less)
                .unwrap_or(false)
        }));
        self
    }

    /// Filter where field is less than or equal to value
    pub fn where_lte(mut self, field: &str, value: Value) -> Self {
        let field = field.to_string();
        self.predicates.push(Box::new(move |doc| {
            extract_field(doc, &field)
                .and_then(|v| v.partial_cmp(&value))
                .map(|ord| ord != std::cmp::Ordering::Greater)
                .unwrap_or(false)
        }));
        self
    }

    /// Filter where array field contains value
    ///
    /// # Example
    ///
    /// ```ignore
    /// query.where_contains("capabilities", Value::String("sensor".into()))
    /// ```
    pub fn where_contains(mut self, field: &str, value: Value) -> Self {
        let field = field.to_string();
        self.predicates.push(Box::new(move |doc| {
            extract_array_contains(doc, &field, &value)
        }));
        self
    }

    /// Filter to only documents with IDs in the given set
    ///
    /// Used for integrating with spatial indices like GeohashIndex.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let nearby_ids = geohash_index.find_near(lat, lon)?;
    /// query.filter_by_ids(&nearby_ids)
    /// ```
    pub fn filter_by_ids(mut self, ids: &[String]) -> Self {
        self.doc_id_filter = Some(ids.iter().cloned().collect());
        self
    }

    /// Sort results by field
    ///
    /// # Example
    ///
    /// ```ignore
    /// query.order_by("timestamp", SortOrder::Desc)
    /// ```
    pub fn order_by(mut self, field: &str, order: SortOrder) -> Self {
        self.sort_field = Some((field.to_string(), order));
        self
    }

    /// Limit number of results
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// Skip first n results (for pagination)
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = n;
        self
    }

    /// Execute the query and return matching documents
    ///
    /// Returns (doc_id, Automerge) tuples for all matching documents.
    pub fn execute(self) -> Result<Vec<(String, Automerge)>> {
        let prefix = format!("{}:", self.collection_name);
        let all_docs = self.store.scan_prefix(&prefix)?;

        // Filter by doc_id_filter if set
        let filtered_by_id: Vec<(String, Automerge)> =
            if let Some(ref id_filter) = self.doc_id_filter {
                all_docs
                    .into_iter()
                    .filter(|(key, _)| {
                        key.strip_prefix(&prefix)
                            .map(|doc_id| id_filter.contains(doc_id))
                            .unwrap_or(false)
                    })
                    .collect()
            } else {
                all_docs
            };

        // Apply predicates
        let mut results: Vec<(String, Automerge)> = filtered_by_id
            .into_iter()
            .filter(|(_, doc)| self.predicates.iter().all(|pred| pred(doc)))
            .map(|(key, doc)| {
                let doc_id = key.strip_prefix(&prefix).unwrap_or(&key).to_string();
                (doc_id, doc)
            })
            .collect();

        // Sort if specified
        if let Some((field, order)) = &self.sort_field {
            results.sort_by(|(_, a), (_, b)| {
                let val_a = extract_field(a, field);
                let val_b = extract_field(b, field);
                let cmp = match (val_a, val_b) {
                    (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                };
                match order {
                    SortOrder::Asc => cmp,
                    SortOrder::Desc => cmp.reverse(),
                }
            });
        }

        // Apply offset and limit
        let results: Vec<(String, Automerge)> = results
            .into_iter()
            .skip(self.offset)
            .take(self.limit.unwrap_or(usize::MAX))
            .collect();

        Ok(results)
    }

    /// Execute and return only document IDs
    pub fn execute_ids(self) -> Result<Vec<String>> {
        let results = self.execute()?;
        Ok(results.into_iter().map(|(id, _)| id).collect())
    }

    /// Count matching documents (without loading full documents)
    pub fn count(self) -> Result<usize> {
        // For now, we execute and count. Could be optimized later.
        Ok(self.execute()?.len())
    }
}

/// Extract a field value from an Automerge document
///
/// Supports nested field paths like "position.lat" and "data.fuel_percent".
#[cfg(feature = "automerge-backend")]
pub fn extract_field(doc: &Automerge, field: &str) -> Option<Value> {
    let parts: Vec<&str> = field.split('.').collect();
    extract_field_recursive(doc, automerge::ROOT, &parts)
}

#[cfg(feature = "automerge-backend")]
fn extract_field_recursive(
    doc: &Automerge,
    obj_id: automerge::ObjId,
    parts: &[&str],
) -> Option<Value> {
    if parts.is_empty() {
        return None;
    }

    let field_name = parts[0];
    let remaining = &parts[1..];

    match doc.get(&obj_id, field_name) {
        Ok(Some((value, _))) => {
            if remaining.is_empty() {
                // Reached the target field
                automerge_value_to_query_value(&value)
            } else {
                // Need to recurse into nested object
                match value {
                    automerge::Value::Object(obj_type) => {
                        if matches!(obj_type, automerge::ObjType::Map) {
                            if let Ok(Some((automerge::Value::Object(_), nested_obj_id))) =
                                doc.get(&obj_id, field_name)
                            {
                                return extract_field_recursive(doc, nested_obj_id, remaining);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
        }
        Ok(None) => None,
        Err(_) => None,
    }
}

/// Convert Automerge Value to Query Value
#[cfg(feature = "automerge-backend")]
fn automerge_value_to_query_value(value: &automerge::Value) -> Option<Value> {
    match value {
        automerge::Value::Scalar(scalar) => match scalar.as_ref() {
            automerge::ScalarValue::Null => Some(Value::Null),
            automerge::ScalarValue::Boolean(b) => Some(Value::Bool(*b)),
            automerge::ScalarValue::Int(i) => Some(Value::Int(*i)),
            automerge::ScalarValue::Uint(u) => Some(Value::Uint(*u)),
            automerge::ScalarValue::F64(f) => Some(Value::Float(*f)),
            automerge::ScalarValue::Str(s) => Some(Value::String(s.to_string())),
            automerge::ScalarValue::Timestamp(t) => Some(Value::Timestamp(*t)),
            automerge::ScalarValue::Bytes(_) => None, // Not comparable
            automerge::ScalarValue::Counter(_) => None,
            automerge::ScalarValue::Unknown { .. } => None,
        },
        automerge::Value::Object(_) => None, // Objects are not scalar values
    }
}

/// Check if an array field contains a value
#[cfg(feature = "automerge-backend")]
fn extract_array_contains(doc: &Automerge, field: &str, target: &Value) -> bool {
    let parts: Vec<&str> = field.split('.').collect();
    extract_array_contains_recursive(doc, automerge::ROOT, &parts, target)
}

#[cfg(feature = "automerge-backend")]
fn extract_array_contains_recursive(
    doc: &Automerge,
    obj_id: automerge::ObjId,
    parts: &[&str],
    target: &Value,
) -> bool {
    if parts.is_empty() {
        return false;
    }

    let field_name = parts[0];
    let remaining = &parts[1..];

    match doc.get(&obj_id, field_name) {
        Ok(Some((value, obj_id_ref))) => {
            if remaining.is_empty() {
                // Check if this is an array containing the target
                match value {
                    automerge::Value::Object(automerge::ObjType::List) => {
                        // Iterate over array elements
                        let len = doc.length(&obj_id_ref);
                        for idx in 0..len {
                            if let Ok(Some((elem_val, _))) = doc.get(&obj_id_ref, idx) {
                                if let Some(query_val) = automerge_value_to_query_value(&elem_val) {
                                    if &query_val == target {
                                        return true;
                                    }
                                }
                            }
                        }
                        false
                    }
                    _ => false,
                }
            } else {
                // Recurse into nested object
                match value {
                    automerge::Value::Object(automerge::ObjType::Map) => {
                        extract_array_contains_recursive(doc, obj_id_ref, remaining, target)
                    }
                    _ => false,
                }
            }
        }
        _ => false,
    }
}

#[cfg(all(test, feature = "automerge-backend"))]
mod tests {
    use super::*;
    use automerge::transaction::Transactable;
    use tempfile::TempDir;

    fn create_test_store() -> (Arc<AutomergeStore>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(AutomergeStore::open(temp_dir.path()).unwrap());
        (store, temp_dir)
    }

    fn create_test_doc(fields: Vec<(&str, automerge::ScalarValue)>) -> Automerge {
        let mut doc = Automerge::new();
        doc.transact(|tx| {
            for (key, value) in fields {
                tx.put(automerge::ROOT, key, value)?;
            }
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();
        doc
    }

    #[test]
    fn test_value_comparison() {
        assert!(Value::Int(5) > Value::Int(3));
        assert!(Value::Float(3.15) > Value::Float(2.72));
        assert!(Value::String("b".into()) > Value::String("a".into()));
        assert!(Value::Bool(true) > Value::Bool(false));

        // Cross-type numeric comparison
        assert!(Value::Int(5) > Value::Uint(3));
        assert!(Value::Float(5.0) > Value::Int(3));
    }

    #[test]
    fn test_extract_field_simple() {
        let doc = create_test_doc(vec![
            ("name", automerge::ScalarValue::Str("test".into())),
            ("count", automerge::ScalarValue::Int(42)),
            ("active", automerge::ScalarValue::Boolean(true)),
        ]);

        assert_eq!(
            extract_field(&doc, "name"),
            Some(Value::String("test".into()))
        );
        assert_eq!(extract_field(&doc, "count"), Some(Value::Int(42)));
        assert_eq!(extract_field(&doc, "active"), Some(Value::Bool(true)));
        assert_eq!(extract_field(&doc, "nonexistent"), None);
    }

    #[test]
    fn test_where_eq() {
        let (store, _temp) = create_test_store();

        // Create test documents
        let doc1 = create_test_doc(vec![
            ("name", automerge::ScalarValue::Str("alpha".into())),
            ("operational", automerge::ScalarValue::Boolean(true)),
        ]);
        let doc2 = create_test_doc(vec![
            ("name", automerge::ScalarValue::Str("beta".into())),
            ("operational", automerge::ScalarValue::Boolean(false)),
        ]);

        store.put("test:doc1", &doc1).unwrap();
        store.put("test:doc2", &doc2).unwrap();

        let results = Query::new(store.clone(), "test")
            .where_eq("operational", Value::Bool(true))
            .execute()
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "doc1");
    }

    #[test]
    fn test_where_gt_lt() {
        let (store, _temp) = create_test_store();

        let doc1 = create_test_doc(vec![
            ("name", automerge::ScalarValue::Str("a".into())),
            ("score", automerge::ScalarValue::Int(10)),
        ]);
        let doc2 = create_test_doc(vec![
            ("name", automerge::ScalarValue::Str("b".into())),
            ("score", automerge::ScalarValue::Int(50)),
        ]);
        let doc3 = create_test_doc(vec![
            ("name", automerge::ScalarValue::Str("c".into())),
            ("score", automerge::ScalarValue::Int(90)),
        ]);

        store.put("test:doc1", &doc1).unwrap();
        store.put("test:doc2", &doc2).unwrap();
        store.put("test:doc3", &doc3).unwrap();

        // Test where_gt
        let results = Query::new(store.clone(), "test")
            .where_gt("score", Value::Int(30))
            .execute()
            .unwrap();
        assert_eq!(results.len(), 2);

        // Test where_lt
        let results = Query::new(store.clone(), "test")
            .where_lt("score", Value::Int(60))
            .execute()
            .unwrap();
        assert_eq!(results.len(), 2);

        // Test combined
        let results = Query::new(store.clone(), "test")
            .where_gt("score", Value::Int(20))
            .where_lt("score", Value::Int(80))
            .execute()
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "doc2");
    }

    #[test]
    fn test_order_by() {
        let (store, _temp) = create_test_store();

        let doc1 = create_test_doc(vec![("priority", automerge::ScalarValue::Int(3))]);
        let doc2 = create_test_doc(vec![("priority", automerge::ScalarValue::Int(1))]);
        let doc3 = create_test_doc(vec![("priority", automerge::ScalarValue::Int(2))]);

        store.put("test:a", &doc1).unwrap();
        store.put("test:b", &doc2).unwrap();
        store.put("test:c", &doc3).unwrap();

        // Ascending order
        let results = Query::new(store.clone(), "test")
            .order_by("priority", SortOrder::Asc)
            .execute()
            .unwrap();
        let priorities: Vec<i64> = results
            .iter()
            .filter_map(|(_, doc)| {
                if let Some(Value::Int(p)) = extract_field(doc, "priority") {
                    Some(p)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(priorities, vec![1, 2, 3]);

        // Descending order
        let results = Query::new(store.clone(), "test")
            .order_by("priority", SortOrder::Desc)
            .execute()
            .unwrap();
        let priorities: Vec<i64> = results
            .iter()
            .filter_map(|(_, doc)| {
                if let Some(Value::Int(p)) = extract_field(doc, "priority") {
                    Some(p)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(priorities, vec![3, 2, 1]);
    }

    #[test]
    fn test_limit_offset() {
        let (store, _temp) = create_test_store();

        for i in 0..10 {
            let doc = create_test_doc(vec![("index", automerge::ScalarValue::Int(i))]);
            store.put(&format!("test:doc{}", i), &doc).unwrap();
        }

        // Test limit
        let results = Query::new(store.clone(), "test")
            .order_by("index", SortOrder::Asc)
            .limit(3)
            .execute()
            .unwrap();
        assert_eq!(results.len(), 3);

        // Test offset
        let results = Query::new(store.clone(), "test")
            .order_by("index", SortOrder::Asc)
            .offset(7)
            .execute()
            .unwrap();
        assert_eq!(results.len(), 3);

        // Test offset + limit (pagination)
        let results = Query::new(store.clone(), "test")
            .order_by("index", SortOrder::Asc)
            .offset(3)
            .limit(2)
            .execute()
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_filter_by_ids() {
        let (store, _temp) = create_test_store();

        for i in 0..5 {
            let doc = create_test_doc(vec![("value", automerge::ScalarValue::Int(i))]);
            store.put(&format!("test:doc{}", i), &doc).unwrap();
        }

        let results = Query::new(store.clone(), "test")
            .filter_by_ids(&["doc1".to_string(), "doc3".to_string()])
            .execute()
            .unwrap();

        assert_eq!(results.len(), 2);
        let ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"doc1"));
        assert!(ids.contains(&"doc3"));
    }

    #[test]
    fn test_where_contains() {
        let (store, _temp) = create_test_store();

        // Create document with array
        let mut doc1 = Automerge::new();
        doc1.transact(|tx| {
            tx.put(automerge::ROOT, "name", "node1")?;
            let caps_id =
                tx.put_object(automerge::ROOT, "capabilities", automerge::ObjType::List)?;
            tx.insert(&caps_id, 0, "sensor")?;
            tx.insert(&caps_id, 1, "comms")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();

        let mut doc2 = Automerge::new();
        doc2.transact(|tx| {
            tx.put(automerge::ROOT, "name", "node2")?;
            let caps_id =
                tx.put_object(automerge::ROOT, "capabilities", automerge::ObjType::List)?;
            tx.insert(&caps_id, 0, "weapon")?;
            Ok::<(), automerge::AutomergeError>(())
        })
        .unwrap();

        store.put("test:node1", &doc1).unwrap();
        store.put("test:node2", &doc2).unwrap();

        let results = Query::new(store.clone(), "test")
            .where_contains("capabilities", Value::String("sensor".into()))
            .execute()
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "node1");
    }

    #[test]
    fn test_execute_ids() {
        let (store, _temp) = create_test_store();

        let doc1 = create_test_doc(vec![("active", automerge::ScalarValue::Boolean(true))]);
        let doc2 = create_test_doc(vec![("active", automerge::ScalarValue::Boolean(true))]);

        store.put("test:a", &doc1).unwrap();
        store.put("test:b", &doc2).unwrap();

        let ids = Query::new(store.clone(), "test")
            .where_eq("active", Value::Bool(true))
            .execute_ids()
            .unwrap();

        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_count() {
        let (store, _temp) = create_test_store();

        for i in 0..5 {
            let doc = create_test_doc(vec![("value", automerge::ScalarValue::Int(i))]);
            store.put(&format!("test:doc{}", i), &doc).unwrap();
        }

        let count = Query::new(store.clone(), "test")
            .where_gt("value", Value::Int(2))
            .count()
            .unwrap();

        assert_eq!(count, 2);
    }
}
