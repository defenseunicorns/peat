//! Core types for the inference pipeline

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A video frame for processing
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Frame sequence number
    pub frame_id: u64,
    /// Frame timestamp
    pub timestamp: DateTime<Utc>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Raw pixel data (RGB, 3 bytes per pixel) - empty for simulated frames
    pub data: Vec<u8>,
    /// Metadata about the frame source
    pub metadata: FrameMetadata,
}

impl VideoFrame {
    /// Create a new video frame
    pub fn new(frame_id: u64, width: u32, height: u32) -> Self {
        Self {
            frame_id,
            timestamp: Utc::now(),
            width,
            height,
            data: Vec::new(),
            metadata: FrameMetadata::default(),
        }
    }

    /// Create a simulated frame (no pixel data, just metadata)
    pub fn simulated(frame_id: u64, width: u32, height: u32) -> Self {
        Self {
            frame_id,
            timestamp: Utc::now(),
            width,
            height,
            data: Vec::new(),
            metadata: FrameMetadata {
                source: "simulated".to_string(),
                ..Default::default()
            },
        }
    }

    /// Set frame metadata
    pub fn with_metadata(mut self, metadata: FrameMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Metadata about a video frame's source
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameMetadata {
    /// Source identifier (camera ID, RTSP URL, etc.)
    pub source: String,
    /// Sensor platform ID (e.g., "Alpha-2")
    pub platform_id: Option<String>,
    /// Geographic position when frame was captured
    pub position: Option<(f64, f64, f64)>,
    /// Sensor bearing in degrees (0 = North)
    pub bearing: Option<f64>,
    /// Horizontal field of view in degrees
    pub hfov: Option<f64>,
}

/// Bounding box in normalized coordinates (0.0 - 1.0)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    /// Left edge (0.0 - 1.0)
    pub x: f32,
    /// Top edge (0.0 - 1.0)
    pub y: f32,
    /// Width (0.0 - 1.0)
    pub width: f32,
    /// Height (0.0 - 1.0)
    pub height: f32,
}

impl BoundingBox {
    /// Create a new bounding box
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Get the center point
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Get the area (for size comparisons)
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// Calculate IoU (Intersection over Union) with another box
    pub fn iou(&self, other: &BoundingBox) -> f32 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);

        if x2 <= x1 || y2 <= y1 {
            return 0.0;
        }

        let intersection = (x2 - x1) * (y2 - y1);
        let union = self.area() + other.area() - intersection;

        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }

    /// Convert to pixel coordinates
    pub fn to_pixels(&self, width: u32, height: u32) -> (i32, i32, i32, i32) {
        let x = (self.x * width as f32) as i32;
        let y = (self.y * height as f32) as i32;
        let w = (self.width * width as f32) as i32;
        let h = (self.height * height as f32) as i32;
        (x, y, w, h)
    }
}

/// Object classification with confidence
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Classification {
    /// Class label (e.g., "person", "vehicle", "bicycle")
    pub label: String,
    /// Class ID (model-specific)
    pub class_id: u32,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

impl Classification {
    /// Create a new classification
    pub fn new(label: impl Into<String>, class_id: u32, confidence: f32) -> Self {
        Self {
            label: label.into(),
            class_id,
            confidence,
        }
    }

    /// Common COCO classes
    pub fn person(confidence: f32) -> Self {
        Self::new("person", 0, confidence)
    }

    pub fn vehicle(confidence: f32) -> Self {
        Self::new("vehicle", 2, confidence) // car in COCO
    }

    pub fn bicycle(confidence: f32) -> Self {
        Self::new("bicycle", 1, confidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box_center() {
        let bbox = BoundingBox::new(0.1, 0.2, 0.3, 0.4);
        let (cx, cy) = bbox.center();
        assert!((cx - 0.25).abs() < 0.001);
        assert!((cy - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_bounding_box_iou() {
        let box1 = BoundingBox::new(0.0, 0.0, 0.5, 0.5);
        let box2 = BoundingBox::new(0.25, 0.25, 0.5, 0.5);

        let iou = box1.iou(&box2);
        // Intersection is 0.25 * 0.25 = 0.0625
        // Union is 0.25 + 0.25 - 0.0625 = 0.4375
        // IoU = 0.0625 / 0.4375 ≈ 0.143
        assert!(iou > 0.14 && iou < 0.15);
    }

    #[test]
    fn test_bounding_box_no_overlap() {
        let box1 = BoundingBox::new(0.0, 0.0, 0.2, 0.2);
        let box2 = BoundingBox::new(0.5, 0.5, 0.2, 0.2);

        assert_eq!(box1.iou(&box2), 0.0);
    }

    #[test]
    fn test_bounding_box_to_pixels() {
        let bbox = BoundingBox::new(0.1, 0.2, 0.3, 0.4);
        let (x, y, w, h) = bbox.to_pixels(1920, 1080);

        assert_eq!(x, 192);
        assert_eq!(y, 216);
        assert_eq!(w, 576);
        assert_eq!(h, 432);
    }

    #[test]
    fn test_video_frame_simulated() {
        let frame = VideoFrame::simulated(42, 1920, 1080);
        assert_eq!(frame.frame_id, 42);
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert!(frame.data.is_empty());
        assert_eq!(frame.metadata.source, "simulated");
    }
}
