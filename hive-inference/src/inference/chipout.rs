//! Chipout extraction module - Detection-triggered image extraction
//!
//! Extracts cropped detection images (chipouts) when specific triggers occur.
//! Chipouts are published to HIVE for consumption by ATAK and TAK Server.
//!
//! ## Trigger Conditions
//!
//! - **NewTrack**: First detection of a new track ID
//! - **Reacquire**: Track lost and found again
//! - **ClassChange**: Classification changed (e.g., unknown → person)
//! - **HighConfidence**: Confidence crosses threshold
//! - **Periodic**: Every N seconds for active tracks
//! - **Manual**: On demand (future: from mission task)

use crate::messages::{
    ChipoutConfig, ChipoutDetection, ChipoutDocument, ChipoutImage, ChipoutTrigger,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tracing::debug;

use super::{BoundingBox, Track, VideoFrame};

/// Chipout extractor - determines when to extract chipouts and creates them
pub struct ChipoutExtractor {
    config: ChipoutConfig,
    /// Track state for trigger evaluation
    track_state: HashMap<String, TrackChipoutState>,
    /// Platform ID for source attribution
    platform_id: String,
    /// Model ID for detection attribution
    model_id: String,
    /// Model version
    model_version: String,
}

/// Per-track state for trigger evaluation
#[derive(Debug, Clone)]
struct TrackChipoutState {
    /// Last classification seen
    last_class: String,
    /// Last confidence seen
    last_confidence: f32,
    /// When track was first seen
    first_seen: DateTime<Utc>,
    /// When last chipout was extracted
    last_chipout_at: Option<DateTime<Utc>>,
    /// Whether track was lost (for reacquire detection)
    was_lost: bool,
    /// Number of chipouts extracted for this track
    chipout_count: u32,
}

impl ChipoutExtractor {
    /// Create a new chipout extractor with the given configuration
    pub fn new(
        config: ChipoutConfig,
        platform_id: impl Into<String>,
        model_id: impl Into<String>,
        model_version: impl Into<String>,
    ) -> Self {
        Self {
            config,
            track_state: HashMap::new(),
            platform_id: platform_id.into(),
            model_id: model_id.into(),
            model_version: model_version.into(),
        }
    }

    /// Create with default configuration
    pub fn with_defaults(
        platform_id: impl Into<String>,
        model_id: impl Into<String>,
        model_version: impl Into<String>,
    ) -> Self {
        Self::new(
            ChipoutConfig::default(),
            platform_id,
            model_id,
            model_version,
        )
    }

    /// Evaluate tracks and extract chipouts where triggered
    ///
    /// Returns a list of chipout documents to be published.
    /// Also updates internal track state for future evaluations.
    pub fn evaluate_and_extract(
        &mut self,
        tracks: &[Track],
        frame: &VideoFrame,
    ) -> Vec<ChipoutDocument> {
        if !self.config.enabled {
            return Vec::new();
        }

        let mut chipouts = Vec::new();

        for track in tracks {
            // Check class filter
            if !self.config.class_filter.is_empty()
                && !self.config.class_filter.contains(&track.class_label)
            {
                continue;
            }

            // Check confidence threshold
            if (track.confidence as f64) < self.config.min_confidence {
                continue;
            }

            // Evaluate triggers
            if let Some(trigger) = self.evaluate_triggers(track) {
                if let Some(chipout) = self.extract_chipout(track, frame, trigger) {
                    chipouts.push(chipout);
                }
            }

            // Update track state
            self.update_track_state(track);
        }

        // Mark tracks not in current frame as potentially lost
        self.mark_missing_tracks(tracks);

        chipouts
    }

    /// Evaluate which trigger (if any) applies to this track
    fn evaluate_triggers(&self, track: &Track) -> Option<ChipoutTrigger> {
        let state = self.track_state.get(&track.id);

        for trigger in &self.config.triggers {
            match trigger {
                ChipoutTrigger::NewTrack => {
                    if state.is_none() {
                        debug!("NewTrack trigger for {}", track.id);
                        return Some(ChipoutTrigger::NewTrack);
                    }
                }
                ChipoutTrigger::Reacquire => {
                    if let Some(s) = state {
                        if s.was_lost {
                            debug!("Reacquire trigger for {}", track.id);
                            return Some(ChipoutTrigger::Reacquire);
                        }
                    }
                }
                ChipoutTrigger::ClassChange => {
                    if let Some(s) = state {
                        if s.last_class != track.class_label {
                            debug!(
                                "ClassChange trigger for {}: {} -> {}",
                                track.id, s.last_class, track.class_label
                            );
                            return Some(ChipoutTrigger::ClassChange);
                        }
                    }
                }
                ChipoutTrigger::HighConfidence => {
                    if let Some(s) = state {
                        // Trigger if confidence crossed threshold upward
                        let threshold = self.config.min_confidence as f32;
                        if s.last_confidence < threshold && track.confidence >= threshold {
                            debug!(
                                "HighConfidence trigger for {}: {:.2} -> {:.2}",
                                track.id, s.last_confidence, track.confidence
                            );
                            return Some(ChipoutTrigger::HighConfidence);
                        }
                    }
                }
                ChipoutTrigger::Periodic => {
                    if self.config.periodic_interval_secs > 0 {
                        if let Some(s) = state {
                            if let Some(last) = s.last_chipout_at {
                                let elapsed = Utc::now().signed_duration_since(last);
                                if elapsed.num_seconds() as u64
                                    >= self.config.periodic_interval_secs
                                {
                                    debug!("Periodic trigger for {}", track.id);
                                    return Some(ChipoutTrigger::Periodic);
                                }
                            }
                        }
                    }
                }
                ChipoutTrigger::Manual => {
                    // Manual triggers are handled externally
                }
            }
        }

        None
    }

    /// Extract a chipout from the frame for the given track
    fn extract_chipout(
        &mut self,
        track: &Track,
        frame: &VideoFrame,
        trigger: ChipoutTrigger,
    ) -> Option<ChipoutDocument> {
        // Convert normalized bbox to pixel coordinates with padding
        let (x, y, w, h) = self.bbox_to_pixels_with_padding(&track.bbox, frame);

        // Create detection info
        let detection = ChipoutDetection::new(
            &track.class_label,
            track.confidence as f64,
            [x as u32, y as u32, w as u32, h as u32],
            [frame.width, frame.height],
            &self.model_id,
            &self.model_version,
        );

        // Extract image data (if frame has pixel data)
        let image = if !frame.data.is_empty() {
            self.extract_image_region(frame, x, y, w, h)
        } else {
            // Simulated frame - create placeholder
            ChipoutImage::placeholder(w as u32, h as u32)
        };

        // Create chipout document
        let chipout = ChipoutDocument::new(&track.id, &self.platform_id, detection, image, trigger);

        // Update last chipout time
        if let Some(state) = self.track_state.get_mut(&track.id) {
            state.last_chipout_at = Some(Utc::now());
            state.chipout_count += 1;
        }

        debug!(
            "Extracted chipout {} for track {} (trigger: {})",
            chipout.chipout_id, track.id, trigger
        );

        Some(chipout)
    }

    /// Convert normalized bounding box to pixel coordinates with padding
    fn bbox_to_pixels_with_padding(
        &self,
        bbox: &BoundingBox,
        frame: &VideoFrame,
    ) -> (i32, i32, i32, i32) {
        let padding = self.config.bbox_padding;

        // Calculate padding in normalized coordinates
        let pad_w = bbox.width * padding;
        let pad_h = bbox.height * padding;

        // Expanded bbox (clamp to frame bounds)
        let x = (bbox.x - pad_w).max(0.0);
        let y = (bbox.y - pad_h).max(0.0);
        let right = (bbox.x + bbox.width + pad_w).min(1.0);
        let bottom = (bbox.y + bbox.height + pad_h).min(1.0);
        let w = right - x;
        let h = bottom - y;

        // Convert to pixels
        let px = (x * frame.width as f32) as i32;
        let py = (y * frame.height as f32) as i32;
        let pw = (w * frame.width as f32) as i32;
        let ph = (h * frame.height as f32) as i32;

        (px, py, pw, ph)
    }

    /// Extract image region from frame data
    ///
    /// For now, this creates a placeholder. In a full implementation,
    /// this would crop the frame and encode to JPEG/PNG.
    fn extract_image_region(
        &self,
        frame: &VideoFrame,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> ChipoutImage {
        // Clamp to frame bounds
        let x = x.max(0) as u32;
        let y = y.max(0) as u32;
        let w = (w as u32).min(frame.width.saturating_sub(x));
        let h = (h as u32).min(frame.height.saturating_sub(y));

        // Apply max size constraints
        let w = w.min(self.config.max_width);
        let h = h.min(self.config.max_height);

        // For frames with actual pixel data, we would:
        // 1. Crop the region from frame.data
        // 2. Resize if needed
        // 3. Encode to JPEG/PNG
        // 4. Base64 encode
        //
        // This requires the `image` crate which is feature-gated.
        // For now, create a placeholder indicating dimensions.

        if frame.data.is_empty() {
            return ChipoutImage::placeholder(w, h);
        }

        // Simplified: encode a small placeholder
        // In production, use image crate to actually crop and encode
        let placeholder_data = format!("placeholder:{}x{}@{},{}", w, h, x, y);
        let base64_data = base64_encode(placeholder_data.as_bytes());

        ChipoutImage::from_base64(self.config.format, w, h, base64_data)
    }

    /// Update track state after evaluation
    fn update_track_state(&mut self, track: &Track) {
        let now = Utc::now();

        self.track_state
            .entry(track.id.clone())
            .and_modify(|s| {
                s.last_class = track.class_label.clone();
                s.last_confidence = track.confidence;
                s.was_lost = false;
            })
            .or_insert(TrackChipoutState {
                last_class: track.class_label.clone(),
                last_confidence: track.confidence,
                first_seen: now,
                last_chipout_at: None,
                was_lost: false,
                chipout_count: 0,
            });
    }

    /// Mark tracks not in current frame as potentially lost
    fn mark_missing_tracks(&mut self, current_tracks: &[Track]) {
        let current_ids: std::collections::HashSet<_> =
            current_tracks.iter().map(|t| &t.id).collect();

        for (id, state) in self.track_state.iter_mut() {
            if !current_ids.contains(id) {
                state.was_lost = true;
            }
        }
    }

    /// Manually trigger chipout extraction for a specific track
    pub fn manual_extract(&mut self, track: &Track, frame: &VideoFrame) -> Option<ChipoutDocument> {
        if !self.config.enabled {
            return None;
        }

        self.extract_chipout(track, frame, ChipoutTrigger::Manual)
    }

    /// Get the number of chipouts extracted for a track
    pub fn chipout_count(&self, track_id: &str) -> u32 {
        self.track_state
            .get(track_id)
            .map(|s| s.chipout_count)
            .unwrap_or(0)
    }

    /// Clear state for tracks that haven't been seen recently
    pub fn prune_old_tracks(&mut self, max_age_secs: u64) {
        let now = Utc::now();
        self.track_state.retain(|_id, state| {
            let age = now.signed_duration_since(state.first_seen);
            age.num_seconds() as u64 <= max_age_secs
        });
    }
}

/// Simple base64 encoding (no external dependency)
fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
    }
    String::from_utf8(buf).unwrap()
}

/// Minimal base64 encoder
struct Base64Encoder<W: std::io::Write> {
    writer: W,
    buffer: [u8; 3],
    buffer_len: usize,
}

impl<W: std::io::Write> Base64Encoder<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            buffer: [0; 3],
            buffer_len: 0,
        }
    }

    fn finish(mut self) -> std::io::Result<()> {
        if self.buffer_len > 0 {
            self.encode_block()?;
        }
        Ok(())
    }

    fn encode_block(&mut self) -> std::io::Result<()> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        let b0 = self.buffer[0] as usize;
        let b1 = self.buffer[1] as usize;
        let b2 = self.buffer[2] as usize;

        let c0 = ALPHABET[b0 >> 2];
        let c1 = ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)];
        let c2 = if self.buffer_len > 1 {
            ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)]
        } else {
            b'='
        };
        let c3 = if self.buffer_len > 2 {
            ALPHABET[b2 & 0x3f]
        } else {
            b'='
        };

        self.writer.write_all(&[c0, c1, c2, c3])?;
        self.buffer = [0; 3];
        self.buffer_len = 0;
        Ok(())
    }
}

impl<W: std::io::Write> std::io::Write for Base64Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &byte in buf {
            self.buffer[self.buffer_len] = byte;
            self.buffer_len += 1;

            if self.buffer_len == 3 {
                self.encode_block()?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::BoundingBox;

    fn create_test_track(id: &str, class: &str, confidence: f32) -> Track {
        Track {
            id: id.to_string(),
            state: crate::inference::TrackState::Confirmed,
            bbox: BoundingBox::new(0.1, 0.2, 0.3, 0.4),
            predicted_bbox: BoundingBox::new(0.1, 0.2, 0.3, 0.4),
            class_label: class.to_string(),
            class_id: 0,
            confidence,
            velocity: (0.0, 0.0),
            embedding: vec![],
            hits: 1,
            time_since_update: 0,
            age: 0,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            history: vec![],
        }
    }

    #[test]
    fn test_new_track_trigger() {
        let mut extractor = ChipoutExtractor::with_defaults("test-platform", "yolov8n", "1.0.0");

        let track = create_test_track("TRACK-001", "person", 0.9);
        let frame = VideoFrame::simulated(1, 1920, 1080);

        let chipouts = extractor.evaluate_and_extract(&[track], &frame);

        assert_eq!(chipouts.len(), 1);
        assert_eq!(chipouts[0].track_id, "TRACK-001");
        assert_eq!(chipouts[0].trigger_reason, ChipoutTrigger::NewTrack);
    }

    #[test]
    fn test_no_trigger_on_second_frame() {
        let mut extractor = ChipoutExtractor::with_defaults("test-platform", "yolov8n", "1.0.0");

        let track = create_test_track("TRACK-001", "person", 0.9);
        let frame = VideoFrame::simulated(1, 1920, 1080);

        // First frame - should trigger
        let chipouts = extractor.evaluate_and_extract(&[track.clone()], &frame);
        assert_eq!(chipouts.len(), 1);

        // Second frame - should NOT trigger (same track, no changes)
        let chipouts = extractor.evaluate_and_extract(&[track], &frame);
        assert_eq!(chipouts.len(), 0);
    }

    #[test]
    fn test_reacquire_trigger() {
        let mut extractor = ChipoutExtractor::with_defaults("test-platform", "yolov8n", "1.0.0");

        let track = create_test_track("TRACK-001", "person", 0.9);
        let frame = VideoFrame::simulated(1, 1920, 1080);

        // First frame
        extractor.evaluate_and_extract(&[track.clone()], &frame);

        // Track disappears (empty frame)
        extractor.evaluate_and_extract(&[], &frame);

        // Track reappears - should trigger Reacquire
        let chipouts = extractor.evaluate_and_extract(&[track], &frame);
        assert_eq!(chipouts.len(), 1);
        assert_eq!(chipouts[0].trigger_reason, ChipoutTrigger::Reacquire);
    }

    #[test]
    fn test_class_change_trigger() {
        let mut extractor = ChipoutExtractor::with_defaults("test-platform", "yolov8n", "1.0.0");

        let mut config = ChipoutConfig::default();
        config.class_filter = vec!["person".to_string(), "vehicle".to_string()];
        config.triggers = vec![ChipoutTrigger::NewTrack, ChipoutTrigger::ClassChange];
        extractor.config = config;

        let track1 = create_test_track("TRACK-001", "person", 0.9);
        let frame = VideoFrame::simulated(1, 1920, 1080);

        // First frame as person
        extractor.evaluate_and_extract(&[track1], &frame);

        // Classification changes to vehicle
        let track2 = create_test_track("TRACK-001", "vehicle", 0.9);
        let chipouts = extractor.evaluate_and_extract(&[track2], &frame);

        assert_eq!(chipouts.len(), 1);
        assert_eq!(chipouts[0].trigger_reason, ChipoutTrigger::ClassChange);
    }

    #[test]
    fn test_confidence_filter() {
        let mut extractor = ChipoutExtractor::with_defaults("test-platform", "yolov8n", "1.0.0");

        // Track with low confidence
        let track = create_test_track("TRACK-001", "person", 0.5);
        let frame = VideoFrame::simulated(1, 1920, 1080);

        // Should not trigger (confidence below threshold of 0.8)
        let chipouts = extractor.evaluate_and_extract(&[track], &frame);
        assert_eq!(chipouts.len(), 0);
    }

    #[test]
    fn test_class_filter() {
        let mut extractor = ChipoutExtractor::with_defaults("test-platform", "yolov8n", "1.0.0");

        let mut config = ChipoutConfig::default();
        config.class_filter = vec!["person".to_string()];
        extractor.config = config;

        // Track with non-matching class
        let track = create_test_track("TRACK-001", "bicycle", 0.9);
        let frame = VideoFrame::simulated(1, 1920, 1080);

        // Should not trigger (bicycle not in filter)
        let chipouts = extractor.evaluate_and_extract(&[track], &frame);
        assert_eq!(chipouts.len(), 0);
    }

    #[test]
    fn test_disabled_extractor() {
        let mut extractor = ChipoutExtractor::with_defaults("test-platform", "yolov8n", "1.0.0");
        extractor.config.enabled = false;

        let track = create_test_track("TRACK-001", "person", 0.9);
        let frame = VideoFrame::simulated(1, 1920, 1080);

        let chipouts = extractor.evaluate_and_extract(&[track], &frame);
        assert_eq!(chipouts.len(), 0);
    }

    #[test]
    fn test_base64_encode() {
        let encoded = base64_encode(b"Hello");
        assert_eq!(encoded, "SGVsbG8=");
    }
}
