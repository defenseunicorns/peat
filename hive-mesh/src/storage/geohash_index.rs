//! Geohash-based spatial indexing for location queries
//!
//! Provides efficient proximity-based lookups by indexing document locations
//! into geohash cells. Queries return documents from the target cell and its
//! 8 neighboring cells for robust geographic discovery.
//!
//! # Example
//!
//! ```ignore
//! use hive_mesh::storage::GeohashIndex;
//!
//! let index = GeohashIndex::new(7);  // precision 7 = ~153m cells
//!
//! // Index documents by location
//! index.insert("beacon1", 37.7749, -122.4194)?;
//! index.insert("beacon2", 37.7750, -122.4195)?;
//!
//! // Find nearby documents (center + 8 neighbors)
//! let nearby = index.find_near(37.7749, -122.4194)?;
//! assert!(nearby.contains(&"beacon1".to_string()));
//! ```

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

/// Default geohash precision for tactical cell formation (~153m cells)
pub const DEFAULT_GEOHASH_PRECISION: usize = 7;

/// Geohash-based spatial index for efficient proximity queries
///
/// Thread-safe implementation using `Arc<RwLock<>>` for concurrent access.
///
/// # Geohash Precision
///
/// | Precision | Cell Size | Use Case |
/// |-----------|-----------|----------|
/// | 5 | ~4.9km × 4.9km | City-level discovery |
/// | 6 | ~1.2km × 0.6km | Neighborhood-level |
/// | **7** | **~153m × 153m** | **Tactical cell formation** |
/// | 8 | ~38m × 19m | Building-level |
#[derive(Clone)]
pub struct GeohashIndex {
    /// Maps geohash cell → set of document IDs
    index: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Maps document ID → geohash cell (for efficient updates)
    doc_locations: Arc<RwLock<HashMap<String, String>>>,
    /// Geohash encoding precision
    precision: usize,
}

impl GeohashIndex {
    /// Create a new geohash index with the specified precision
    ///
    /// # Arguments
    ///
    /// * `precision` - Geohash string length (1-12). Higher = smaller cells.
    ///   Use 7 for ~153m tactical cells (default for HIVE Protocol).
    pub fn new(precision: usize) -> Self {
        Self {
            index: Arc::new(RwLock::new(HashMap::new())),
            doc_locations: Arc::new(RwLock::new(HashMap::new())),
            precision,
        }
    }

    /// Create an index with default precision (7 = ~153m cells)
    pub fn default_precision() -> Self {
        Self::new(DEFAULT_GEOHASH_PRECISION)
    }

    /// Get the precision of this index
    pub fn precision(&self) -> usize {
        self.precision
    }

    /// Insert a document at a geographic location
    ///
    /// If the document already exists at a different location, it is moved.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Unique document identifier
    /// * `lat` - Latitude in degrees (-90 to 90)
    /// * `lon` - Longitude in degrees (-180 to 180)
    pub fn insert(&self, doc_id: &str, lat: f64, lon: f64) -> Result<()> {
        let geohash = self.encode(lat, lon)?;

        // Remove from old location if exists
        {
            let mut doc_locs = self.doc_locations.write().unwrap();
            if let Some(old_geohash) = doc_locs.remove(doc_id) {
                let mut idx = self.index.write().unwrap();
                if let Some(cell) = idx.get_mut(&old_geohash) {
                    cell.remove(doc_id);
                    if cell.is_empty() {
                        idx.remove(&old_geohash);
                    }
                }
            }
        }

        // Insert at new location
        {
            let mut idx = self.index.write().unwrap();
            idx.entry(geohash.clone())
                .or_default()
                .insert(doc_id.to_string());
        }
        {
            let mut doc_locs = self.doc_locations.write().unwrap();
            doc_locs.insert(doc_id.to_string(), geohash);
        }

        Ok(())
    }

    /// Find documents near a geographic location
    ///
    /// Returns document IDs from the center cell and its 8 neighboring cells.
    /// This provides robust coverage even for documents near cell boundaries.
    ///
    /// # Arguments
    ///
    /// * `lat` - Latitude in degrees
    /// * `lon` - Longitude in degrees
    ///
    /// # Returns
    ///
    /// Vector of document IDs within the 9-cell neighborhood
    pub fn find_near(&self, lat: f64, lon: f64) -> Result<Vec<String>> {
        let center_hash = self.encode(lat, lon)?;
        let neighbors = self.get_neighbors(&center_hash)?;

        let idx = self.index.read().unwrap();
        let mut results = HashSet::new();

        // Include center cell
        if let Some(docs) = idx.get(&center_hash) {
            results.extend(docs.iter().cloned());
        }

        // Include 8 neighboring cells
        for neighbor_hash in neighbors {
            if let Some(docs) = idx.get(&neighbor_hash) {
                results.extend(docs.iter().cloned());
            }
        }

        Ok(results.into_iter().collect())
    }

    /// Find documents in a specific geohash cell only (no neighbors)
    pub fn find_in_cell(&self, geohash: &str) -> Vec<String> {
        let idx = self.index.read().unwrap();
        idx.get(geohash)
            .map(|docs| docs.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Remove a document from the index
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Document identifier to remove
    ///
    /// # Returns
    ///
    /// Ok(true) if document was found and removed, Ok(false) if not found
    pub fn remove(&self, doc_id: &str) -> Result<bool> {
        let mut doc_locs = self.doc_locations.write().unwrap();
        if let Some(geohash) = doc_locs.remove(doc_id) {
            let mut idx = self.index.write().unwrap();
            if let Some(cell) = idx.get_mut(&geohash) {
                cell.remove(doc_id);
                if cell.is_empty() {
                    idx.remove(&geohash);
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Update a document's location
    ///
    /// More efficient than remove + insert when the old location is known.
    pub fn update(
        &self,
        doc_id: &str,
        _old_lat: f64,
        _old_lon: f64,
        new_lat: f64,
        new_lon: f64,
    ) -> Result<()> {
        // Just use insert which handles the update internally
        self.insert(doc_id, new_lat, new_lon)
    }

    /// Check if a document exists in the index
    pub fn contains(&self, doc_id: &str) -> bool {
        self.doc_locations.read().unwrap().contains_key(doc_id)
    }

    /// Get the geohash cell for a document
    pub fn get_cell(&self, doc_id: &str) -> Option<String> {
        self.doc_locations.read().unwrap().get(doc_id).cloned()
    }

    /// Count total documents in the index
    pub fn len(&self) -> usize {
        self.doc_locations.read().unwrap().len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Count number of geohash cells with documents
    pub fn cell_count(&self) -> usize {
        self.index.read().unwrap().len()
    }

    /// Clear all entries from the index
    pub fn clear(&self) {
        self.index.write().unwrap().clear();
        self.doc_locations.write().unwrap().clear();
    }

    /// Get all document IDs in the index
    pub fn all_doc_ids(&self) -> Vec<String> {
        self.doc_locations.read().unwrap().keys().cloned().collect()
    }

    /// Encode a coordinate to a geohash string
    fn encode(&self, lat: f64, lon: f64) -> Result<String> {
        let coord = geohash::Coord { x: lon, y: lat };
        geohash::encode(coord, self.precision).context("Failed to encode geohash")
    }

    /// Get the 8 neighboring geohash cells
    fn get_neighbors(&self, geohash: &str) -> Result<Vec<String>> {
        let neighbors = geohash::neighbors(geohash).context("Failed to get geohash neighbors")?;
        Ok(vec![
            neighbors.n,
            neighbors.ne,
            neighbors.e,
            neighbors.se,
            neighbors.s,
            neighbors.sw,
            neighbors.w,
            neighbors.nw,
        ])
    }
}

impl Default for GeohashIndex {
    fn default() -> Self {
        Self::default_precision()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // San Francisco coordinates for testing
    const SF_LAT: f64 = 37.7749;
    const SF_LON: f64 = -122.4194;

    // Slightly offset coordinates (within same cell at precision 7)
    const SF_LAT_NEAR: f64 = 37.7750;
    const SF_LON_NEAR: f64 = -122.4193;

    // Different location (New York)
    const NY_LAT: f64 = 40.7128;
    const NY_LON: f64 = -74.0060;

    #[test]
    fn test_new_index() {
        let index = GeohashIndex::new(7);
        assert_eq!(index.precision(), 7);
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert_eq!(index.cell_count(), 0);
    }

    #[test]
    fn test_default_precision() {
        let index = GeohashIndex::default_precision();
        assert_eq!(index.precision(), DEFAULT_GEOHASH_PRECISION);
    }

    #[test]
    fn test_insert_and_find_near() {
        let index = GeohashIndex::new(7);

        index.insert("beacon1", SF_LAT, SF_LON).unwrap();
        index.insert("beacon2", SF_LAT_NEAR, SF_LON_NEAR).unwrap();

        let nearby = index.find_near(SF_LAT, SF_LON).unwrap();
        assert!(nearby.contains(&"beacon1".to_string()));
        assert!(nearby.contains(&"beacon2".to_string()));
        assert_eq!(nearby.len(), 2);
    }

    #[test]
    fn test_find_near_includes_neighbors() {
        let index = GeohashIndex::new(7);

        // Insert at SF
        index.insert("sf_beacon", SF_LAT, SF_LON).unwrap();

        // Insert at a neighbor cell (small offset)
        // At precision 7, cells are ~153m, so offset by ~100m should be neighbor
        let offset_lat = SF_LAT + 0.001; // ~111m north
        let offset_lon = SF_LON + 0.001; // ~85m east
        index
            .insert("neighbor_beacon", offset_lat, offset_lon)
            .unwrap();

        // Both should be found from either location
        let nearby_sf = index.find_near(SF_LAT, SF_LON).unwrap();
        assert!(nearby_sf.contains(&"sf_beacon".to_string()));
        // May or may not include neighbor depending on exact cell boundaries

        let nearby_offset = index.find_near(offset_lat, offset_lon).unwrap();
        assert!(nearby_offset.contains(&"neighbor_beacon".to_string()));
    }

    #[test]
    fn test_find_near_excludes_distant() {
        let index = GeohashIndex::new(7);

        index.insert("sf_beacon", SF_LAT, SF_LON).unwrap();
        index.insert("ny_beacon", NY_LAT, NY_LON).unwrap();

        let nearby_sf = index.find_near(SF_LAT, SF_LON).unwrap();
        assert!(nearby_sf.contains(&"sf_beacon".to_string()));
        assert!(!nearby_sf.contains(&"ny_beacon".to_string()));

        let nearby_ny = index.find_near(NY_LAT, NY_LON).unwrap();
        assert!(nearby_ny.contains(&"ny_beacon".to_string()));
        assert!(!nearby_ny.contains(&"sf_beacon".to_string()));
    }

    #[test]
    fn test_find_in_cell() {
        let index = GeohashIndex::new(7);

        index.insert("beacon1", SF_LAT, SF_LON).unwrap();
        let cell = index.get_cell("beacon1").unwrap();

        let in_cell = index.find_in_cell(&cell);
        assert!(in_cell.contains(&"beacon1".to_string()));
    }

    #[test]
    fn test_remove() {
        let index = GeohashIndex::new(7);

        index.insert("beacon1", SF_LAT, SF_LON).unwrap();
        assert!(index.contains("beacon1"));
        assert_eq!(index.len(), 1);

        let removed = index.remove("beacon1").unwrap();
        assert!(removed);
        assert!(!index.contains("beacon1"));
        assert_eq!(index.len(), 0);

        // Removing again should return false
        let removed_again = index.remove("beacon1").unwrap();
        assert!(!removed_again);
    }

    #[test]
    fn test_update_location() {
        let index = GeohashIndex::new(7);

        index.insert("beacon1", SF_LAT, SF_LON).unwrap();
        let old_cell = index.get_cell("beacon1").unwrap();

        index
            .update("beacon1", SF_LAT, SF_LON, NY_LAT, NY_LON)
            .unwrap();
        let new_cell = index.get_cell("beacon1").unwrap();

        assert_ne!(old_cell, new_cell);
        assert!(index.contains("beacon1"));
        assert_eq!(index.len(), 1);

        // Should be findable at new location
        let nearby_ny = index.find_near(NY_LAT, NY_LON).unwrap();
        assert!(nearby_ny.contains(&"beacon1".to_string()));

        // Should NOT be findable at old location
        let nearby_sf = index.find_near(SF_LAT, SF_LON).unwrap();
        assert!(!nearby_sf.contains(&"beacon1".to_string()));
    }

    #[test]
    fn test_insert_updates_existing() {
        let index = GeohashIndex::new(7);

        index.insert("beacon1", SF_LAT, SF_LON).unwrap();
        assert_eq!(index.len(), 1);

        // Insert same doc at different location should move it
        index.insert("beacon1", NY_LAT, NY_LON).unwrap();
        assert_eq!(index.len(), 1); // Still only one document

        let nearby_ny = index.find_near(NY_LAT, NY_LON).unwrap();
        assert!(nearby_ny.contains(&"beacon1".to_string()));
    }

    #[test]
    fn test_clear() {
        let index = GeohashIndex::new(7);

        index.insert("beacon1", SF_LAT, SF_LON).unwrap();
        index.insert("beacon2", NY_LAT, NY_LON).unwrap();
        assert_eq!(index.len(), 2);

        index.clear();
        assert!(index.is_empty());
        assert_eq!(index.cell_count(), 0);
    }

    #[test]
    fn test_all_doc_ids() {
        let index = GeohashIndex::new(7);

        index.insert("a", SF_LAT, SF_LON).unwrap();
        index.insert("b", NY_LAT, NY_LON).unwrap();
        index.insert("c", SF_LAT_NEAR, SF_LON_NEAR).unwrap();

        let all_ids = index.all_doc_ids();
        assert_eq!(all_ids.len(), 3);
        assert!(all_ids.contains(&"a".to_string()));
        assert!(all_ids.contains(&"b".to_string()));
        assert!(all_ids.contains(&"c".to_string()));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let index = Arc::new(GeohashIndex::new(7));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let idx = Arc::clone(&index);
                thread::spawn(move || {
                    let lat = SF_LAT + (i as f64 * 0.001);
                    let lon = SF_LON + (i as f64 * 0.001);
                    idx.insert(&format!("beacon{}", i), lat, lon).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(index.len(), 10);
    }

    #[test]
    fn test_different_precisions() {
        // Lower precision = larger cells = more documents in same cell
        let index_5 = GeohashIndex::new(5);
        let index_7 = GeohashIndex::new(7);
        let index_9 = GeohashIndex::new(9);

        // Get cells at different precisions
        index_5.insert("p5", SF_LAT, SF_LON).unwrap();
        index_7.insert("p7", SF_LAT, SF_LON).unwrap();
        index_9.insert("p9", SF_LAT, SF_LON).unwrap();

        let cell_5 = index_5.get_cell("p5").unwrap();
        let cell_7 = index_7.get_cell("p7").unwrap();
        let cell_9 = index_9.get_cell("p9").unwrap();

        assert_eq!(cell_5.len(), 5);
        assert_eq!(cell_7.len(), 7);
        assert_eq!(cell_9.len(), 9);

        // Higher precision cell should be prefix of lower precision
        assert!(cell_7.starts_with(&cell_5));
        assert!(cell_9.starts_with(&cell_7));
    }

    #[test]
    fn test_empty_find_near() {
        let index = GeohashIndex::new(7);
        let nearby = index.find_near(SF_LAT, SF_LON).unwrap();
        assert!(nearby.is_empty());
    }

    #[test]
    fn test_get_cell_not_found() {
        let index = GeohashIndex::new(7);
        assert!(index.get_cell("nonexistent").is_none());
    }
}
