//! Metrics collection and reporting for E2E tests
//!
//! Collects timing, bandwidth, and success metrics during test execution.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Message types for bandwidth tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    /// Capability advertisement
    CapabilityAdvertisement,
    /// Track update from AI
    TrackUpdate,
    /// Command from C2
    Command,
    /// Handoff message
    Handoff,
    /// Model update package metadata
    ModelUpdateMeta,
    /// Model update binary data
    ModelUpdateData,
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageType::CapabilityAdvertisement => write!(f, "Capability Ads"),
            MessageType::TrackUpdate => write!(f, "Track Updates"),
            MessageType::Command => write!(f, "Commands"),
            MessageType::Handoff => write!(f, "Handoff"),
            MessageType::ModelUpdateMeta => write!(f, "Model Meta"),
            MessageType::ModelUpdateData => write!(f, "Model Data"),
        }
    }
}

/// Collected metrics from E2E test run
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    /// Test start time
    start_time: Option<Instant>,

    /// Team formation duration
    pub formation_duration: Option<Duration>,

    /// Command latencies (C2 → Platform acknowledgment)
    pub command_latencies: Vec<Duration>,

    /// Track update latencies (AI detection → C2 receipt)
    pub track_latencies: Vec<Duration>,

    /// Handoff gap times (release → acquisition)
    pub handoff_gaps: Vec<Duration>,

    /// Capability sync times
    pub sync_times: Vec<Duration>,

    /// Bandwidth usage per message type (in bytes)
    pub bandwidth: HashMap<MessageType, usize>,

    /// Message counts per type
    pub message_counts: HashMap<MessageType, usize>,

    /// Phase completion timestamps
    pub phase_completions: Vec<(String, Duration)>,

    /// Errors encountered
    pub errors: Vec<String>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            start_time: None,
            formation_duration: None,
            command_latencies: Vec::new(),
            track_latencies: Vec::new(),
            handoff_gaps: Vec::new(),
            sync_times: Vec::new(),
            bandwidth: HashMap::new(),
            message_counts: HashMap::new(),
            phase_completions: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Start the metrics timer
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Get elapsed time since start
    pub fn elapsed(&self) -> Duration {
        self.start_time
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// Record team formation completion
    pub fn record_formation(&mut self, duration: Duration) {
        self.formation_duration = Some(duration);
        self.record_phase("Formation", duration);
    }

    /// Record a command latency measurement
    pub fn record_command_latency(&mut self, latency: Duration) {
        self.command_latencies.push(latency);
    }

    /// Record a track update latency measurement
    pub fn record_track_latency(&mut self, latency: Duration) {
        self.track_latencies.push(latency);
    }

    /// Record a handoff gap measurement
    pub fn record_handoff_gap(&mut self, gap: Duration) {
        self.handoff_gaps.push(gap);
    }

    /// Record a sync time measurement
    pub fn record_sync_time(&mut self, duration: Duration) {
        self.sync_times.push(duration);
    }

    /// Record bandwidth usage for a message
    pub fn record_message(&mut self, msg_type: MessageType, size_bytes: usize) {
        *self.bandwidth.entry(msg_type).or_insert(0) += size_bytes;
        *self.message_counts.entry(msg_type).or_insert(0) += 1;
    }

    /// Record phase completion
    pub fn record_phase(&mut self, phase: &str, duration: Duration) {
        self.phase_completions.push((phase.to_string(), duration));
    }

    /// Record an error
    pub fn record_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
    }

    /// Calculate average of durations
    fn avg_duration(durations: &[Duration]) -> Option<Duration> {
        if durations.is_empty() {
            None
        } else {
            let total: Duration = durations.iter().sum();
            Some(total / durations.len() as u32)
        }
    }

    /// Generate the metrics report
    pub fn report(&self) -> MetricsReport {
        MetricsReport {
            total_duration: self.elapsed(),
            formation_duration: self.formation_duration,
            avg_command_latency: Self::avg_duration(&self.command_latencies),
            avg_track_latency: Self::avg_duration(&self.track_latencies),
            avg_handoff_gap: Self::avg_duration(&self.handoff_gaps),
            total_bandwidth: self.bandwidth.values().sum(),
            bandwidth_by_type: self.bandwidth.clone(),
            message_counts: self.message_counts.clone(),
            phases: self.phase_completions.clone(),
            errors: self.errors.clone(),
        }
    }
}

/// Summary report of E2E test metrics
#[derive(Debug, Clone)]
pub struct MetricsReport {
    /// Total test duration
    pub total_duration: Duration,

    /// Team formation duration
    pub formation_duration: Option<Duration>,

    /// Average command latency
    pub avg_command_latency: Option<Duration>,

    /// Average track update latency
    pub avg_track_latency: Option<Duration>,

    /// Average handoff gap
    pub avg_handoff_gap: Option<Duration>,

    /// Total bandwidth used (bytes)
    pub total_bandwidth: usize,

    /// Bandwidth by message type
    pub bandwidth_by_type: HashMap<MessageType, usize>,

    /// Message counts by type
    pub message_counts: HashMap<MessageType, usize>,

    /// Phase completion times
    pub phases: Vec<(String, Duration)>,

    /// Errors encountered
    pub errors: Vec<String>,
}

impl MetricsReport {
    /// Check if formation met target (< 30 seconds)
    pub fn formation_ok(&self) -> bool {
        self.formation_duration
            .map(|d| d < Duration::from_secs(30))
            .unwrap_or(false)
    }

    /// Check if command latency met target (< 2 seconds)
    pub fn command_latency_ok(&self) -> bool {
        self.avg_command_latency
            .map(|d| d < Duration::from_secs(2))
            .unwrap_or(true) // OK if no commands sent
    }

    /// Check if track latency met target (< 2 seconds)
    pub fn track_latency_ok(&self) -> bool {
        self.avg_track_latency
            .map(|d| d < Duration::from_secs(2))
            .unwrap_or(true)
    }

    /// Check if handoff gap met target (< 5 seconds)
    pub fn handoff_gap_ok(&self) -> bool {
        self.avg_handoff_gap
            .map(|d| d < Duration::from_secs(5))
            .unwrap_or(true)
    }

    /// Check if all metrics met targets
    pub fn all_ok(&self) -> bool {
        self.formation_ok()
            && self.command_latency_ok()
            && self.track_latency_ok()
            && self.handoff_gap_ok()
            && self.errors.is_empty()
    }

    /// Format duration for display
    fn fmt_duration(d: Option<Duration>) -> String {
        match d {
            Some(d) => format!("{:.1}s", d.as_secs_f64()),
            None => "N/A".to_string(),
        }
    }

    /// Format bytes for display
    fn fmt_bytes(bytes: usize) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    /// Generate formatted report string
    pub fn to_string_pretty(&self) -> String {
        let mut s = String::new();

        s.push_str("══════════════════════════════════════════════════════════════\n");
        s.push_str("                    M1 VIGNETTE E2E REPORT\n");
        s.push_str("══════════════════════════════════════════════════════════════\n\n");

        // Timing metrics
        s.push_str("TIMING METRICS\n");
        s.push_str(&format!(
            "  Team Formation:     {:8}  (target: < 30s)  {}\n",
            Self::fmt_duration(self.formation_duration),
            if self.formation_ok() { "✓" } else { "✗" }
        ));
        s.push_str(&format!(
            "  Command Latency:    {:8}  (target: < 2s)   {}\n",
            Self::fmt_duration(self.avg_command_latency),
            if self.command_latency_ok() {
                "✓"
            } else {
                "✗"
            }
        ));
        s.push_str(&format!(
            "  Track Update:       {:8}  (target: < 2s)   {}\n",
            Self::fmt_duration(self.avg_track_latency),
            if self.track_latency_ok() {
                "✓"
            } else {
                "✗"
            }
        ));
        s.push_str(&format!(
            "  Handoff Gap:        {:8}  (target: < 5s)   {}\n",
            Self::fmt_duration(self.avg_handoff_gap),
            if self.handoff_gap_ok() { "✓" } else { "✗" }
        ));

        // Bandwidth comparison
        s.push_str("\nBANDWIDTH COMPARISON\n");
        s.push_str("  Message Type          Peat        Traditional (Video)\n");
        s.push_str("  ─────────────────────────────────────────────────────\n");

        let track_bytes = self
            .bandwidth_by_type
            .get(&MessageType::TrackUpdate)
            .copied()
            .unwrap_or(0);
        s.push_str(&format!(
            "  Track Updates         {:10}  5 Mbps continuous\n",
            Self::fmt_bytes(track_bytes)
        ));

        let cmd_bytes = self
            .bandwidth_by_type
            .get(&MessageType::Command)
            .copied()
            .unwrap_or(0);
        s.push_str(&format!(
            "  Commands              {:10}  ~200 B\n",
            Self::fmt_bytes(cmd_bytes)
        ));

        let cap_bytes = self
            .bandwidth_by_type
            .get(&MessageType::CapabilityAdvertisement)
            .copied()
            .unwrap_or(0);
        s.push_str(&format!(
            "  Capability Ads        {:10}  N/A\n",
            Self::fmt_bytes(cap_bytes)
        ));

        let model_bytes = self
            .bandwidth_by_type
            .get(&MessageType::ModelUpdateData)
            .copied()
            .unwrap_or(0);
        s.push_str(&format!(
            "  Model Updates         {:10}  ~45 MB\n",
            Self::fmt_bytes(model_bytes)
        ));

        s.push_str("  ─────────────────────────────────────────────────────\n");
        s.push_str(&format!(
            "  Total                 {:10}  ~2.25 GB/hr (video)\n",
            Self::fmt_bytes(self.total_bandwidth)
        ));

        // Phase completions
        if !self.phases.is_empty() {
            s.push_str("\nPHASE COMPLETIONS\n");
            for (phase, duration) in &self.phases {
                s.push_str(&format!(
                    "  [✓] {}: {:.1}s\n",
                    phase,
                    duration.as_secs_f64()
                ));
            }
        }

        // Errors
        if !self.errors.is_empty() {
            s.push_str("\nERRORS\n");
            for error in &self.errors {
                s.push_str(&format!("  [✗] {}\n", error));
            }
        }

        s.push_str("\n══════════════════════════════════════════════════════════════\n");

        s
    }
}

impl std::fmt::Display for MetricsReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_pretty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collection() {
        let mut metrics = MetricsCollector::new();
        metrics.start();

        metrics.record_formation(Duration::from_secs(15));
        metrics.record_command_latency(Duration::from_millis(800));
        metrics.record_command_latency(Duration::from_millis(1200));
        metrics.record_track_latency(Duration::from_millis(500));
        metrics.record_message(MessageType::TrackUpdate, 512);
        metrics.record_message(MessageType::TrackUpdate, 480);

        let report = metrics.report();

        assert!(report.formation_ok());
        assert!(report.command_latency_ok());
        assert_eq!(
            report.message_counts.get(&MessageType::TrackUpdate),
            Some(&2)
        );
        assert_eq!(
            report.bandwidth_by_type.get(&MessageType::TrackUpdate),
            Some(&992)
        );
    }

    #[test]
    fn test_metrics_report_format() {
        let mut metrics = MetricsCollector::new();
        metrics.start();
        metrics.record_formation(Duration::from_secs(12));
        metrics.record_track_latency(Duration::from_millis(1500));
        metrics.record_message(MessageType::TrackUpdate, 500);

        let report = metrics.report();
        let output = report.to_string_pretty();

        assert!(output.contains("M1 VIGNETTE E2E REPORT"));
        assert!(output.contains("Team Formation"));
        assert!(output.contains("✓")); // Should have checkmark for good formation time
    }
}
