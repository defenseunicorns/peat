//! Performance metrics for the inference pipeline

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Latency statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LatencyStats {
    /// Minimum latency in milliseconds
    pub min_ms: f64,
    /// Maximum latency in milliseconds
    pub max_ms: f64,
    /// Mean latency in milliseconds
    pub mean_ms: f64,
    /// Median latency (P50)
    pub p50_ms: f64,
    /// 95th percentile latency
    pub p95_ms: f64,
    /// 99th percentile latency
    pub p99_ms: f64,
    /// Sample count
    pub count: usize,
}

impl LatencyStats {
    /// Calculate stats from samples
    pub fn from_samples(samples: &[f64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }

        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let count = sorted.len();
        let sum: f64 = sorted.iter().sum();

        Self {
            min_ms: sorted[0],
            max_ms: sorted[count - 1],
            mean_ms: sum / count as f64,
            p50_ms: sorted[count / 2],
            p95_ms: sorted[(count as f64 * 0.95) as usize],
            p99_ms: sorted[(count as f64 * 0.99).min(count as f64 - 1.0) as usize],
            count,
        }
    }
}

/// Inference metrics collector
#[derive(Debug)]
pub struct InferenceMetrics {
    /// Detection latencies (ms)
    detection_latencies: VecDeque<f64>,
    /// Tracking latencies (ms)
    tracking_latencies: VecDeque<f64>,
    /// Total pipeline latencies (ms)
    pipeline_latencies: VecDeque<f64>,
    /// Frames per second samples
    fps_samples: VecDeque<f64>,
    /// Detection counts per frame
    detection_counts: VecDeque<usize>,
    /// Track counts per frame
    track_counts: VecDeque<usize>,
    /// Maximum samples to keep
    max_samples: usize,
    /// Start time
    start_time: Instant,
    /// Last frame time
    last_frame_time: Option<Instant>,
    /// Total frames processed
    total_frames: u64,
    /// Total detections
    total_detections: u64,
    /// Current detection timer
    detection_start: Option<Instant>,
    /// Current tracking timer
    tracking_start: Option<Instant>,
    /// Current pipeline timer
    pipeline_start: Option<Instant>,
}

impl Default for InferenceMetrics {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl InferenceMetrics {
    /// Create a new metrics collector
    pub fn new(max_samples: usize) -> Self {
        Self {
            detection_latencies: VecDeque::with_capacity(max_samples),
            tracking_latencies: VecDeque::with_capacity(max_samples),
            pipeline_latencies: VecDeque::with_capacity(max_samples),
            fps_samples: VecDeque::with_capacity(max_samples),
            detection_counts: VecDeque::with_capacity(max_samples),
            track_counts: VecDeque::with_capacity(max_samples),
            max_samples,
            start_time: Instant::now(),
            last_frame_time: None,
            total_frames: 0,
            total_detections: 0,
            detection_start: None,
            tracking_start: None,
            pipeline_start: None,
        }
    }

    /// Start timing a detection operation
    pub fn start_detection(&mut self) {
        self.detection_start = Some(Instant::now());
    }

    /// End timing a detection operation
    pub fn end_detection(&mut self, detection_count: usize) {
        if let Some(start) = self.detection_start.take() {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            if self.detection_latencies.len() >= self.max_samples {
                self.detection_latencies.pop_front();
            }
            self.detection_latencies.push_back(latency);

            if self.detection_counts.len() >= self.max_samples {
                self.detection_counts.pop_front();
            }
            self.detection_counts.push_back(detection_count);

            self.total_detections += detection_count as u64;
        }
    }

    /// Start timing a tracking operation
    pub fn start_tracking(&mut self) {
        self.tracking_start = Some(Instant::now());
    }

    /// End timing a tracking operation
    pub fn end_tracking(&mut self, track_count: usize) {
        if let Some(start) = self.tracking_start.take() {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            if self.tracking_latencies.len() >= self.max_samples {
                self.tracking_latencies.pop_front();
            }
            self.tracking_latencies.push_back(latency);

            if self.track_counts.len() >= self.max_samples {
                self.track_counts.pop_front();
            }
            self.track_counts.push_back(track_count);
        }
    }

    /// Start timing full pipeline
    pub fn start_pipeline(&mut self) {
        self.pipeline_start = Some(Instant::now());
    }

    /// End timing full pipeline
    pub fn end_pipeline(&mut self) {
        if let Some(start) = self.pipeline_start.take() {
            let latency = start.elapsed().as_secs_f64() * 1000.0;
            self.add_pipeline_latency(latency);
        }
    }

    /// Add a pipeline latency measurement
    fn add_pipeline_latency(&mut self, latency_ms: f64) {
        if self.pipeline_latencies.len() >= self.max_samples {
            self.pipeline_latencies.pop_front();
        }
        self.pipeline_latencies.push_back(latency_ms);
    }

    /// Helper to push a sample to a queue
    fn push_sample(&self, queue: &mut VecDeque<f64>, value: f64) {
        if queue.len() >= self.max_samples {
            queue.pop_front();
        }
        queue.push_back(value);
    }

    /// Record a frame (for FPS calculation)
    pub fn record_frame(&mut self) {
        self.total_frames += 1;

        if let Some(last) = self.last_frame_time {
            let delta = last.elapsed().as_secs_f64();
            if delta > 0.0 {
                let fps = 1.0 / delta;
                if self.fps_samples.len() >= self.max_samples {
                    self.fps_samples.pop_front();
                }
                self.fps_samples.push_back(fps);
            }
        }
        self.last_frame_time = Some(Instant::now());
    }

    /// Record detection latency directly
    pub fn record_detection_latency(&mut self, latency_ms: f64, count: usize) {
        if self.detection_latencies.len() >= self.max_samples {
            self.detection_latencies.pop_front();
        }
        self.detection_latencies.push_back(latency_ms);

        if self.detection_counts.len() >= self.max_samples {
            self.detection_counts.pop_front();
        }
        self.detection_counts.push_back(count);

        self.total_detections += count as u64;
    }

    /// Record tracking latency directly
    pub fn record_tracking_latency(&mut self, latency_ms: f64, count: usize) {
        if self.tracking_latencies.len() >= self.max_samples {
            self.tracking_latencies.pop_front();
        }
        self.tracking_latencies.push_back(latency_ms);

        if self.track_counts.len() >= self.max_samples {
            self.track_counts.pop_front();
        }
        self.track_counts.push_back(count);
    }

    /// Record pipeline latency directly
    pub fn record_pipeline_latency(&mut self, latency_ms: f64) {
        self.add_pipeline_latency(latency_ms);
    }

    /// Get detection latency statistics
    pub fn detection_latency(&self) -> LatencyStats {
        let samples: Vec<f64> = self.detection_latencies.iter().copied().collect();
        LatencyStats::from_samples(&samples)
    }

    /// Get tracking latency statistics
    pub fn tracking_latency(&self) -> LatencyStats {
        let samples: Vec<f64> = self.tracking_latencies.iter().copied().collect();
        LatencyStats::from_samples(&samples)
    }

    /// Get pipeline latency statistics
    pub fn pipeline_latency(&self) -> LatencyStats {
        let samples: Vec<f64> = self.pipeline_latencies.iter().copied().collect();
        LatencyStats::from_samples(&samples)
    }

    /// Get current FPS
    pub fn current_fps(&self) -> f64 {
        self.fps_samples.back().copied().unwrap_or(0.0)
    }

    /// Get average FPS
    pub fn average_fps(&self) -> f64 {
        if self.fps_samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.fps_samples.iter().sum();
        sum / self.fps_samples.len() as f64
    }

    /// Get total frames processed
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// Get total detections
    pub fn total_detections(&self) -> u64 {
        self.total_detections
    }

    /// Get average detections per frame
    pub fn avg_detections_per_frame(&self) -> f64 {
        if self.detection_counts.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.detection_counts.iter().map(|&x| x as f64).sum();
        sum / self.detection_counts.len() as f64
    }

    /// Get average tracks per frame
    pub fn avg_tracks_per_frame(&self) -> f64 {
        if self.track_counts.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.track_counts.iter().map(|&x| x as f64).sum();
        sum / self.track_counts.len() as f64
    }

    /// Get uptime
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get a summary report
    pub fn summary(&self) -> MetricsSummary {
        MetricsSummary {
            uptime_secs: self.uptime().as_secs_f64(),
            total_frames: self.total_frames,
            total_detections: self.total_detections,
            avg_fps: self.average_fps(),
            current_fps: self.current_fps(),
            detection_latency: self.detection_latency(),
            tracking_latency: self.tracking_latency(),
            pipeline_latency: self.pipeline_latency(),
            avg_detections_per_frame: self.avg_detections_per_frame(),
            avg_tracks_per_frame: self.avg_tracks_per_frame(),
            timestamp: Utc::now(),
        }
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        self.detection_latencies.clear();
        self.tracking_latencies.clear();
        self.pipeline_latencies.clear();
        self.fps_samples.clear();
        self.detection_counts.clear();
        self.track_counts.clear();
        self.start_time = Instant::now();
        self.last_frame_time = None;
        self.total_frames = 0;
        self.total_detections = 0;
    }
}

/// Summary of inference metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// Uptime in seconds
    pub uptime_secs: f64,
    /// Total frames processed
    pub total_frames: u64,
    /// Total detections
    pub total_detections: u64,
    /// Average FPS
    pub avg_fps: f64,
    /// Current FPS
    pub current_fps: f64,
    /// Detection latency stats
    pub detection_latency: LatencyStats,
    /// Tracking latency stats
    pub tracking_latency: LatencyStats,
    /// Pipeline latency stats
    pub pipeline_latency: LatencyStats,
    /// Average detections per frame
    pub avg_detections_per_frame: f64,
    /// Average tracks per frame
    pub avg_tracks_per_frame: f64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Inference Metrics Summary ===")?;
        writeln!(f, "Uptime: {:.1}s", self.uptime_secs)?;
        writeln!(f, "Frames: {}", self.total_frames)?;
        writeln!(f, "Detections: {}", self.total_detections)?;
        writeln!(
            f,
            "FPS: {:.1} (current: {:.1})",
            self.avg_fps, self.current_fps
        )?;
        writeln!(
            f,
            "Detection latency: {:.1}ms (P95: {:.1}ms)",
            self.detection_latency.mean_ms, self.detection_latency.p95_ms
        )?;
        writeln!(
            f,
            "Tracking latency: {:.1}ms (P95: {:.1}ms)",
            self.tracking_latency.mean_ms, self.tracking_latency.p95_ms
        )?;
        writeln!(
            f,
            "Pipeline latency: {:.1}ms (P95: {:.1}ms)",
            self.pipeline_latency.mean_ms, self.pipeline_latency.p95_ms
        )?;
        writeln!(
            f,
            "Avg detections/frame: {:.1}",
            self.avg_detections_per_frame
        )?;
        writeln!(f, "Avg tracks/frame: {:.1}", self.avg_tracks_per_frame)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_stats() {
        let samples = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let stats = LatencyStats::from_samples(&samples);

        assert_eq!(stats.min_ms, 10.0);
        assert_eq!(stats.max_ms, 50.0);
        assert_eq!(stats.mean_ms, 30.0);
        assert_eq!(stats.count, 5);
    }

    #[test]
    fn test_latency_stats_empty() {
        let stats = LatencyStats::from_samples(&[]);
        assert_eq!(stats.count, 0);
        assert_eq!(stats.mean_ms, 0.0);
    }

    #[test]
    fn test_metrics_recording() {
        let mut metrics = InferenceMetrics::new(100);

        metrics.record_detection_latency(50.0, 3);
        metrics.record_detection_latency(60.0, 2);

        assert_eq!(metrics.total_detections(), 5);
        let stats = metrics.detection_latency();
        assert_eq!(stats.count, 2);
    }

    #[test]
    fn test_metrics_fps() {
        let mut metrics = InferenceMetrics::new(100);

        // Simulate frames
        metrics.record_frame();
        std::thread::sleep(std::time::Duration::from_millis(50));
        metrics.record_frame();
        std::thread::sleep(std::time::Duration::from_millis(50));
        metrics.record_frame();

        // Should be around 20 FPS (50ms between frames)
        let fps = metrics.average_fps();
        assert!(fps > 10.0 && fps < 30.0);
    }

    #[test]
    fn test_metrics_reset() {
        let mut metrics = InferenceMetrics::new(100);
        metrics.record_detection_latency(50.0, 5);
        metrics.record_frame();

        metrics.reset();

        assert_eq!(metrics.total_frames(), 0);
        assert_eq!(metrics.total_detections(), 0);
    }
}
