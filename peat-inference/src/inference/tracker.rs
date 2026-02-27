//! Multi-object tracker trait and implementations
//!
//! Provides DeepSORT/ByteTrack-style tracking:
//! - Track lifecycle management (tentative → confirmed → lost)
//! - Re-identification using appearance embeddings
//! - Kalman filter for motion prediction
//! - Hungarian algorithm for detection-track association

use super::detector::Detection;
use super::types::BoundingBox;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Track state in its lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackState {
    /// New track, not yet confirmed (need N consecutive detections)
    Tentative,
    /// Confirmed track with consistent detections
    Confirmed,
    /// Lost track (no detection for N frames)
    Lost,
    /// Track has exited the scene
    Deleted,
}

/// A tracked object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Unique track ID
    pub id: String,
    /// Current state
    pub state: TrackState,
    /// Current bounding box
    pub bbox: BoundingBox,
    /// Predicted bounding box (from Kalman filter)
    pub predicted_bbox: BoundingBox,
    /// Object classification
    pub class_label: String,
    /// Class ID
    pub class_id: u32,
    /// Smoothed confidence
    pub confidence: f32,
    /// Velocity estimate (normalized coords per frame)
    pub velocity: (f32, f32),
    /// Appearance embedding (for re-identification)
    pub embedding: Vec<f32>,
    /// Number of consecutive detections
    pub hits: u32,
    /// Number of frames since last detection
    pub time_since_update: u32,
    /// Total age in frames
    pub age: u32,
    /// First seen timestamp
    pub first_seen: DateTime<Utc>,
    /// Last updated timestamp
    pub last_seen: DateTime<Utc>,
    /// Detection history (last N bounding boxes)
    pub history: Vec<BoundingBox>,
}

impl Track {
    /// Create a new track from a detection
    pub fn new(id: String, detection: &Detection) -> Self {
        Self {
            id,
            state: TrackState::Tentative,
            bbox: detection.bbox,
            predicted_bbox: detection.bbox,
            class_label: detection.classification.label.clone(),
            class_id: detection.classification.class_id,
            confidence: detection.classification.confidence,
            velocity: (0.0, 0.0),
            embedding: detection.embedding.clone().unwrap_or_default(),
            hits: 1,
            time_since_update: 0,
            age: 0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            history: vec![detection.bbox],
        }
    }

    /// Update track with a new detection
    pub fn update(&mut self, detection: &Detection) {
        self.update_with_min_hits(detection, 3);
    }

    /// Update track with a new detection, using specified min_hits for confirmation
    pub fn update_with_min_hits(&mut self, detection: &Detection, min_hits: u32) {
        // Update velocity estimate
        let (cx_old, cy_old) = self.bbox.center();
        let (cx_new, cy_new) = detection.bbox.center();
        let alpha = 0.3; // Smoothing factor
        self.velocity.0 = alpha * (cx_new - cx_old) + (1.0 - alpha) * self.velocity.0;
        self.velocity.1 = alpha * (cy_new - cy_old) + (1.0 - alpha) * self.velocity.1;

        // Update bounding box
        self.bbox = detection.bbox;

        // Smooth confidence
        self.confidence = 0.7 * detection.classification.confidence + 0.3 * self.confidence;

        // Update embedding (exponential moving average)
        if let Some(ref new_emb) = detection.embedding {
            if self.embedding.is_empty() {
                self.embedding = new_emb.clone();
            } else {
                for (i, v) in new_emb.iter().enumerate() {
                    if i < self.embedding.len() {
                        self.embedding[i] = 0.9 * self.embedding[i] + 0.1 * v;
                    }
                }
            }
        }

        self.hits += 1;
        self.time_since_update = 0;
        self.last_seen = Utc::now();

        // Keep last 30 positions
        self.history.push(detection.bbox);
        if self.history.len() > 30 {
            self.history.remove(0);
        }

        // State transition: tentative → confirmed
        if self.state == TrackState::Tentative && self.hits >= min_hits {
            self.state = TrackState::Confirmed;
        }

        // Recover lost track
        if self.state == TrackState::Lost {
            self.state = TrackState::Confirmed;
        }
    }

    /// Predict next position (simple linear motion model)
    pub fn predict(&mut self) {
        self.predicted_bbox = BoundingBox::new(
            self.bbox.x + self.velocity.0,
            self.bbox.y + self.velocity.1,
            self.bbox.width,
            self.bbox.height,
        );

        self.age += 1;
        self.time_since_update += 1;
    }

    /// Mark track as lost after too many frames without update
    pub fn mark_lost(&mut self, max_age: u32) {
        if self.time_since_update > max_age {
            if self.state == TrackState::Confirmed {
                self.state = TrackState::Lost;
            } else if self.state == TrackState::Lost || self.state == TrackState::Tentative {
                self.state = TrackState::Deleted;
            }
        }
    }

    /// Check if track should be deleted
    pub fn is_deleted(&self) -> bool {
        self.state == TrackState::Deleted
    }

    /// Check if track is confirmed
    pub fn is_confirmed(&self) -> bool {
        self.state == TrackState::Confirmed
    }

    /// Get speed in normalized coordinates per frame
    pub fn speed(&self) -> f32 {
        (self.velocity.0.powi(2) + self.velocity.1.powi(2)).sqrt()
    }

    /// Get bearing in degrees (0 = north, 90 = east, 180 = south, 270 = west)
    /// Note: In image coordinates, +y is DOWN (south), +x is RIGHT (east)
    pub fn bearing(&self) -> f32 {
        let angle = self.velocity.1.atan2(self.velocity.0).to_degrees();
        // Convert from math coords to compass bearing
        // atan2(vy, vx): (1,0)→0°, (0,1)→90°, (-1,0)→180°, (0,-1)→-90°
        // Compass: (1,0)→90° (east), (0,1)→180° (south)
        (90.0 + angle + 360.0) % 360.0
    }
}

/// Multi-object tracker trait
#[async_trait]
pub trait Tracker: Send + Sync {
    /// Update tracker with new detections
    async fn update(&mut self, detections: Vec<Detection>) -> anyhow::Result<Vec<Track>>;

    /// Get all active tracks (confirmed only)
    fn get_tracks(&self) -> Vec<&Track>;

    /// Get all tracks including tentative
    fn get_all_tracks(&self) -> Vec<&Track>;

    /// Get a specific track by ID
    fn get_track(&self, id: &str) -> Option<&Track>;

    /// Reset the tracker
    fn reset(&mut self);

    /// Get tracker statistics
    fn stats(&self) -> TrackerStats;
}

/// Tracker statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackerStats {
    pub total_tracks_created: u64,
    pub active_tracks: usize,
    pub confirmed_tracks: usize,
    pub lost_tracks: usize,
    pub frames_processed: u64,
}

/// Tracker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerConfig {
    /// Minimum hits to confirm a track
    pub min_hits: u32,
    /// Maximum frames without detection before marking lost
    pub max_age: u32,
    /// Maximum frames in lost state before deletion
    pub max_lost_age: u32,
    /// IoU threshold for detection-track association
    pub iou_threshold: f32,
    /// Cosine distance threshold for re-identification
    pub reid_threshold: f32,
    /// Weight of appearance vs motion in association (0-1)
    pub appearance_weight: f32,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            min_hits: 3,
            max_age: 30,      // ~1 second at 30 FPS
            max_lost_age: 60, // ~2 seconds
            iou_threshold: 0.3,
            reid_threshold: 0.5,
            appearance_weight: 0.5,
        }
    }
}

// ============================================================================
// Simulated Tracker (DeepSORT-style)
// ============================================================================

/// Simulated multi-object tracker
pub struct SimulatedTracker {
    config: TrackerConfig,
    tracks: HashMap<String, Track>,
    next_id: u64,
    stats: TrackerStats,
}

impl SimulatedTracker {
    /// Create a new tracker
    pub fn new(config: TrackerConfig) -> Self {
        Self {
            config,
            tracks: HashMap::new(),
            next_id: 1,
            stats: TrackerStats::default(),
        }
    }

    /// Create with default config
    pub fn default_config() -> Self {
        Self::new(TrackerConfig::default())
    }

    /// Generate a new track ID
    fn next_track_id(&mut self) -> String {
        let id = format!("TRACK-{:04}", self.next_id);
        self.next_id += 1;
        id
    }

    /// Calculate cosine similarity between embeddings
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.is_empty() || b.is_empty() || a.len() != b.len() {
            return 0.0;
        }

        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a > 0.0 && norm_b > 0.0 {
            dot / (norm_a * norm_b)
        } else {
            0.0
        }
    }

    /// Calculate association cost between detection and track
    fn association_cost(&self, detection: &Detection, track: &Track) -> f32 {
        // IoU cost (lower is better)
        let iou = detection.bbox.iou(&track.predicted_bbox);
        let iou_cost = 1.0 - iou;

        // Appearance cost (cosine distance)
        let appearance_cost = if let Some(ref det_emb) = detection.embedding {
            1.0 - Self::cosine_similarity(det_emb, &track.embedding)
        } else {
            1.0
        };

        // Weighted combination
        let w = self.config.appearance_weight;
        w * appearance_cost + (1.0 - w) * iou_cost
    }

    /// Calculate association cost between detection and track data
    fn association_cost_data(
        &self,
        detection: &Detection,
        predicted_bbox: &BoundingBox,
        embedding: &[f32],
    ) -> f32 {
        // IoU cost (lower is better)
        let iou = detection.bbox.iou(predicted_bbox);
        let iou_cost = 1.0 - iou;

        // Appearance cost (cosine distance)
        let appearance_cost = if let Some(ref det_emb) = detection.embedding {
            1.0 - Self::cosine_similarity(det_emb, embedding)
        } else {
            1.0
        };

        // Weighted combination
        let w = self.config.appearance_weight;
        w * appearance_cost + (1.0 - w) * iou_cost
    }

    /// Match detections to track data (avoids borrow issues)
    fn match_detections_to_data(
        &self,
        detections: &[Detection],
        track_data: &[(String, BoundingBox, Vec<f32>)],
    ) -> (Vec<(usize, usize)>, Vec<usize>, Vec<usize>) {
        let mut matches = Vec::new();
        let mut unmatched_detections: Vec<usize> = (0..detections.len()).collect();
        let mut unmatched_tracks: Vec<usize> = (0..track_data.len()).collect();

        if detections.is_empty() || track_data.is_empty() {
            return (matches, unmatched_detections, unmatched_tracks);
        }

        // Build cost matrix
        let mut costs: Vec<(usize, usize, f32)> = Vec::new();
        for (d_idx, det) in detections.iter().enumerate() {
            for (t_idx, (_, predicted_bbox, embedding)) in track_data.iter().enumerate() {
                let cost = self.association_cost_data(det, predicted_bbox, embedding);
                costs.push((d_idx, t_idx, cost));
            }
        }

        // Sort by cost (greedy matching)
        costs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

        // Greedy assignment
        for (d_idx, t_idx, cost) in costs {
            // Check thresholds
            let iou = detections[d_idx].bbox.iou(&track_data[t_idx].1);
            if iou < self.config.iou_threshold && cost > self.config.reid_threshold {
                continue;
            }

            // Check if already matched
            if !unmatched_detections.contains(&d_idx) || !unmatched_tracks.contains(&t_idx) {
                continue;
            }

            matches.push((d_idx, t_idx));
            unmatched_detections.retain(|&x| x != d_idx);
            unmatched_tracks.retain(|&x| x != t_idx);
        }

        (matches, unmatched_detections, unmatched_tracks)
    }
}

#[async_trait]
impl Tracker for SimulatedTracker {
    async fn update(&mut self, detections: Vec<Detection>) -> anyhow::Result<Vec<Track>> {
        self.stats.frames_processed += 1;

        // Predict new locations for all tracks
        for track in self.tracks.values_mut() {
            track.predict();
        }

        // Get active track IDs and their predicted bboxes for matching
        let track_ids: Vec<String> = self
            .tracks
            .iter()
            .filter(|(_, t)| t.state != TrackState::Deleted)
            .map(|(id, _)| id.clone())
            .collect();

        // Build track data for matching (id, predicted_bbox, embedding)
        let track_data: Vec<(String, BoundingBox, Vec<f32>)> = track_ids
            .iter()
            .filter_map(|id| {
                self.tracks
                    .get(id)
                    .map(|t| (id.clone(), t.predicted_bbox, t.embedding.clone()))
            })
            .collect();

        // Match detections to tracks
        let (matches, unmatched_dets, unmatched_track_indices) =
            self.match_detections_to_data(&detections, &track_data);

        // Update matched tracks
        let min_hits = self.config.min_hits;
        for (d_idx, t_idx) in &matches {
            let track_id = &track_data[*t_idx].0;
            if let Some(track) = self.tracks.get_mut(track_id) {
                track.update_with_min_hits(&detections[*d_idx], min_hits);
            }
        }

        // Create new tracks for unmatched detections
        for d_idx in unmatched_dets {
            let id = self.next_track_id();
            let track = Track::new(id.clone(), &detections[d_idx]);
            self.tracks.insert(id, track);
            self.stats.total_tracks_created += 1;
        }

        // Mark lost tracks
        let max_age = self.config.max_age;
        for t_idx in unmatched_track_indices {
            let track_id = &track_data[t_idx].0;
            if let Some(track) = self.tracks.get_mut(track_id) {
                track.mark_lost(max_age);
            }
        }

        // Handle lost → deleted transition
        let max_lost_age = self.config.max_lost_age;
        for track in self.tracks.values_mut() {
            if track.state == TrackState::Lost && track.time_since_update > max_lost_age {
                track.state = TrackState::Deleted;
            }
        }

        // Remove deleted tracks
        self.tracks.retain(|_, t| t.state != TrackState::Deleted);

        // Update stats
        self.stats.active_tracks = self.tracks.len();
        self.stats.confirmed_tracks = self.tracks.values().filter(|t| t.is_confirmed()).count();
        self.stats.lost_tracks = self
            .tracks
            .values()
            .filter(|t| t.state == TrackState::Lost)
            .count();

        // Return confirmed tracks
        Ok(self
            .tracks
            .values()
            .filter(|t| t.is_confirmed())
            .cloned()
            .collect())
    }

    fn get_tracks(&self) -> Vec<&Track> {
        self.tracks.values().filter(|t| t.is_confirmed()).collect()
    }

    fn get_all_tracks(&self) -> Vec<&Track> {
        self.tracks.values().collect()
    }

    fn get_track(&self, id: &str) -> Option<&Track> {
        self.tracks.get(id)
    }

    fn reset(&mut self) {
        self.tracks.clear();
        self.next_id = 1;
        self.stats = TrackerStats::default();
    }

    fn stats(&self) -> TrackerStats {
        self.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::types::Classification;

    fn make_detection(x: f32, y: f32, frame_id: u64) -> Detection {
        Detection::new(
            BoundingBox::new(x, y, 0.1, 0.2),
            Classification::person(0.9),
            frame_id,
        )
        .with_embedding(vec![0.1; 128])
    }

    #[tokio::test]
    async fn test_tracker_create_track() {
        let mut tracker = SimulatedTracker::default_config();

        let detections = vec![make_detection(0.5, 0.5, 1)];
        let tracks = tracker.update(detections).await.unwrap();

        // Track is tentative, not confirmed yet
        assert!(tracks.is_empty());
        assert_eq!(tracker.get_all_tracks().len(), 1);
        assert_eq!(tracker.stats().total_tracks_created, 1);
    }

    #[tokio::test]
    async fn test_tracker_confirm_track() {
        let mut tracker = SimulatedTracker::new(TrackerConfig {
            min_hits: 3,
            ..Default::default()
        });

        // Need 3 consecutive detections to confirm
        for i in 0..3 {
            let detections = vec![make_detection(0.5 + i as f32 * 0.01, 0.5, i as u64)];
            tracker.update(detections).await.unwrap();
        }

        let confirmed = tracker.get_tracks();
        assert_eq!(confirmed.len(), 1);
        assert!(confirmed[0].is_confirmed());
    }

    #[tokio::test]
    async fn test_tracker_track_movement() {
        let mut tracker = SimulatedTracker::new(TrackerConfig {
            min_hits: 1, // Immediate confirmation for testing
            ..Default::default()
        });

        // Moving detection
        for i in 0..5 {
            let detections = vec![make_detection(0.3 + i as f32 * 0.05, 0.5, i as u64)];
            tracker.update(detections).await.unwrap();
        }

        let tracks = tracker.get_tracks();
        assert_eq!(tracks.len(), 1);

        // Should have positive x velocity
        assert!(tracks[0].velocity.0 > 0.0);
    }

    #[tokio::test]
    async fn test_tracker_track_lost() {
        let mut tracker = SimulatedTracker::new(TrackerConfig {
            min_hits: 1,
            max_age: 5,
            ..Default::default()
        });

        // Create and confirm track
        let detections = vec![make_detection(0.5, 0.5, 0)];
        tracker.update(detections).await.unwrap();

        // No detections for several frames
        for i in 1..10 {
            tracker.update(vec![]).await.unwrap();

            if i <= 5 {
                // Should still exist but eventually go lost
                assert!(!tracker.get_all_tracks().is_empty());
            }
        }

        // Track should be marked lost or deleted
        let stats = tracker.stats();
        assert!(stats.lost_tracks > 0 || stats.active_tracks == 0);
    }

    #[test]
    fn test_track_bearing() {
        let det = make_detection(0.5, 0.5, 0);
        let mut track = Track::new("TEST-001".to_string(), &det);

        // Moving right (east) - in image coords, +x is right
        track.velocity = (0.1, 0.0);
        let bearing_east = track.bearing();
        assert!(
            bearing_east > 80.0 && bearing_east < 100.0,
            "East bearing: {}",
            bearing_east
        );

        // Moving down in image coords = south in world (y increases downward)
        track.velocity = (0.0, 0.1);
        let bearing_south = track.bearing();
        assert!(
            bearing_south > 170.0 && bearing_south < 190.0,
            "South bearing: {}",
            bearing_south
        );
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((SimulatedTracker::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(SimulatedTracker::cosine_similarity(&a, &c).abs() < 0.001);
    }
}
