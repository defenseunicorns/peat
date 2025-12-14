//! Metrics event types and logging for HIVE simulation
//!
//! This module provides structured logging for simulation metrics including:
//! - Document synchronization latency
//! - Aggregation events
//! - Command dissemination tracking

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Global metrics file handle for persistent logging
static METRICS_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

/// Metrics event for JSON logging
#[derive(Debug, serde::Serialize)]
#[serde(tag = "event_type")]
pub enum MetricsEvent {
    DocumentInserted {
        node_id: String,
        doc_id: String,
        timestamp_us: u128, // Unix timestamp in microseconds
    },
    DocumentReceived {
        node_id: String,
        doc_id: String,
        created_at_us: u128,    // When document was first created
        last_modified_us: u128, // When document was last updated
        received_at_us: u128,   // When we received it
        latency_us: u128,       // Propagation time
        latency_ms: f64,
        version: u64,             // Document version
        is_first_reception: bool, // true = creation sync, false = update/recovery sync
        latency_type: String,     // "creation", "update", or "recovery"
    },
    MessageSent {
        node_id: String,
        node_type: String,
        message_number: u64,
        message_size_bytes: usize,
        timestamp_us: u128,
    },
    DocumentAcknowledged {
        node_id: String,
        doc_id: String,
        timestamp_us: u128,
    },
    #[allow(dead_code)] // Reserved for future ack tracking feature
    AllAcksReceived {
        node_id: String,
        doc_id: String,
        inserted_at_us: u128,
        all_acked_at_us: u128,
        round_trip_latency_us: u128,
        round_trip_latency_ms: f64,
        ack_count: usize,
    },
    SquadSummaryCreated {
        node_id: String,
        squad_id: String,
        member_count: usize,
        readiness_score: f64,
        timestamp_us: u128,
    },
    PlatoonSummaryCreated {
        node_id: String,
        platoon_id: String,
        squad_count: usize,
        total_member_count: usize,
        timestamp_us: u128,
    },
    CompanySummaryCreated {
        node_id: String,
        company_id: String,
        platoon_count: u32,
        total_member_count: u32,
        timestamp_us: u128,
    },
    // Phase 3: Command dissemination events
    CommandIssued {
        node_id: String,
        command_id: String,
        target_scope: String, // "Node", "Squad", "Platoon", "Battalion"
        target_ids: Vec<String>,
        priority: i32,
        timestamp_us: u128,
    },
    CommandReceived {
        node_id: String,
        command_id: String,
        originator_id: String,
        received_at_us: u128,
        latency_us: u128,
        latency_ms: f64,
    },
    CommandAcknowledged {
        node_id: String,
        command_id: String,
        status: String, // "RECEIVED", "COMPLETED", "FAILED"
        timestamp_us: u128,
    },
    AcknowledgmentReceived {
        node_id: String, // Originator who receives the ack
        command_id: String,
        ack_from_node_id: String, // Subordinate who sent ack
        status: String,           // "RECEIVED", "COMPLETED", "FAILED"
        timestamp_us: u128,
        ack_count: usize,          // How many acks received so far
        expected_ack_count: usize, // Total expected acks
    },
    #[allow(dead_code)] // Will be used for round-trip latency tracking
    AllCommandAcksReceived {
        node_id: String,
        command_id: String,
        issued_at_us: u128,
        all_acked_at_us: u128,
        round_trip_latency_us: u128,
        round_trip_latency_ms: f64,
        ack_count: usize,
    },
    // Phase 4: Propagation latency tracking events
    AggregationStarted {
        node_id: String,
        tier: String,           // "squad", "platoon", "company"
        input_doc_type: String, // What we're aggregating (NodeState, SquadSummary, etc.)
        input_count: usize,     // How many documents we're aggregating
        timestamp_us: u128,
    },
    AggregationCompleted {
        node_id: String,
        tier: String,
        input_doc_type: String,
        output_doc_type: String, // What we produced (SquadSummary, PlatoonSummary, etc.)
        output_doc_id: String,
        input_count: usize,
        processing_time_us: u128, // Time spent aggregating
        timestamp_us: u128,
    },
}

/// Initialize the metrics file for persistent logging
/// Called once at startup to create/open the log file based on NODE_ID
pub fn init_metrics_file() {
    METRICS_FILE.get_or_init(|| {
        let node_id = std::env::var("NODE_ID").unwrap_or_else(|_| "unknown".to_string());
        let log_dir = PathBuf::from("/data/logs");

        // Try to create the log directory (may fail if not mounted, that's ok)
        if std::fs::create_dir_all(&log_dir).is_ok() {
            let log_path = log_dir.join(format!("{}.metrics.log", node_id));
            match OpenOptions::new().create(true).append(true).open(&log_path) {
                Ok(file) => {
                    eprintln!("[{}] Metrics logging to: {:?}", node_id, log_path);
                    Mutex::new(Some(file))
                }
                Err(e) => {
                    eprintln!(
                        "[{}] Warning: Could not open metrics file {:?}: {}",
                        node_id, log_path, e
                    );
                    Mutex::new(None)
                }
            }
        } else {
            eprintln!(
                "[{}] Warning: /data/logs not available, using stderr only",
                node_id
            );
            Mutex::new(None)
        }
    });
}

/// Log metrics event as JSON to stderr and persistent file (for parsing)
pub fn log_metrics(event: &MetricsEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        let line = format!("METRICS: {}", json);

        // Always write to stderr for backward compatibility
        eprintln!("{}", line);

        // Also write to persistent file if available
        if let Some(file_mutex) = METRICS_FILE.get() {
            if let Ok(mut guard) = file_mutex.lock() {
                if let Some(ref mut file) = *guard {
                    // Write with newline and flush immediately
                    let _ = writeln!(file, "{}", line);
                    let _ = file.flush();
                }
            }
        }
    }
}
