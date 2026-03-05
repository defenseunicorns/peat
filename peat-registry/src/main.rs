//! peat-registry — OCI registry sync control plane for DDIL environments
//!
//! Orchestrates digest-level delta sync with checkpoint/resume, topology-aware
//! routing, and CRDT-synced convergence tracking between OCI registries (ADR-054).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tracing::{error, info, warn};

use peat_registry::config::RegistryConfig;
use peat_registry::convergence::drift;
use peat_registry::convergence::referrers;
use peat_registry::convergence::ConvergenceTracker;
use peat_registry::delta;
use peat_registry::oci::{OciRegistryClient, RegistryClient};
use peat_registry::scheduler::budget::BudgetManager;
use peat_registry::scheduler::wave::WaveController;
use peat_registry::topology::selector::select_source;
use peat_registry::topology::RegistryGraph;
use peat_registry::transfer::checkpoint::CheckpointStore;
use peat_registry::transfer::engine::TransferEngine;
use peat_registry::types::ConvergenceStatus;

/// PEAT Registry Sync — OCI registry synchronization for DDIL environments
#[derive(Parser, Debug)]
#[command(name = "peat-registry")]
#[command(about = "OCI registry sync control plane for DDIL environments")]
struct Args {
    /// Path to TOML configuration file
    #[arg(long, env = "PEAT_REGISTRY_CONFIG", default_value = "registry.toml")]
    config: PathBuf,

    /// Data directory for checkpoints and state
    #[arg(long, env = "PEAT_REGISTRY_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Sync loop interval in seconds
    #[arg(long, env = "PEAT_REGISTRY_SYNC_INTERVAL")]
    sync_interval: Option<u64>,

    /// Drift detection interval in seconds
    #[arg(long, env = "PEAT_REGISTRY_DRIFT_INTERVAL")]
    drift_interval: Option<u64>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let filter = if args.verbose {
        "peat_registry=debug"
    } else {
        "peat_registry=info"
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("peat-registry starting");

    // Load config
    let mut config = RegistryConfig::load(&args.config)?;

    // Override from CLI args
    if let Some(data_dir) = args.data_dir {
        config.data_dir = data_dir;
    }
    if let Some(interval) = args.sync_interval {
        config.sync_interval_secs = interval;
    }
    if let Some(interval) = args.drift_interval {
        config.drift_interval_secs = interval;
    }

    info!("data_dir: {:?}", config.data_dir);
    info!("sync_interval: {}s", config.sync_interval_secs);
    info!("drift_interval: {}s", config.drift_interval_secs);

    // Create data directory
    std::fs::create_dir_all(&config.data_dir)?;

    // Build topology graph
    let graph = RegistryGraph::new(config.targets.clone(), &config.edges)?;
    info!(
        "topology: {} targets, {} edges, max_wave={}",
        graph.targets.len(),
        graph.edges.values().map(|v| v.len()).sum::<usize>(),
        graph.max_wave()
    );

    // Create OCI clients for each target
    let mut clients: HashMap<String, Arc<dyn RegistryClient>> = HashMap::new();
    for target in &config.targets {
        let client = OciRegistryClient::new(target.clone());
        clients.insert(target.id.clone(), Arc::new(client));
    }

    // Open checkpoint store
    let checkpoint_path = config.data_dir.join("checkpoints.redb");
    let checkpoint_store = Arc::new(CheckpointStore::open(&checkpoint_path)?);

    // Initialize convergence tracker
    let tracker = Arc::new(ConvergenceTracker::new());

    // Initialize budget manager
    let budget_mgr = Arc::new(BudgetManager::new());
    for edges in graph.edges.values() {
        for edge in edges {
            if let Some(budget) = edge.bandwidth_budget_bytes_per_hour {
                budget_mgr.register_edge(&edge.parent_id, &edge.child_id, budget);
            }
        }
    }

    // Initialize wave controller
    let wave_ctrl = WaveController::new(config.wave.gate_threshold);

    info!("peat-registry initialized, entering sync loop");

    // Main sync loop
    let sync_interval = tokio::time::Duration::from_secs(config.sync_interval_secs);
    let drift_interval = tokio::time::Duration::from_secs(config.drift_interval_secs);

    let mut sync_timer = tokio::time::interval(sync_interval);
    let mut drift_timer = tokio::time::interval(drift_interval);

    loop {
        tokio::select! {
            _ = sync_timer.tick() => {
                for intent in &config.intents {
                    // Process each target in the intent
                    for target_id in &intent.targets {
                        // Check wave gating
                        let target_wave = graph.wave_assignments.get(target_id).copied().unwrap_or(0);
                        let convergence_for_intent = tracker.get_states_for_intent(&intent.intent_id);
                        if !wave_ctrl.is_wave_active(target_wave, &graph.wave_assignments, &convergence_for_intent) {
                            info!(intent_id = %intent.intent_id, target_id, wave = target_wave, "wave not active, skipping");
                            continue;
                        }

                        // Select source
                        let source_id = match select_source(&graph, target_id, &convergence_for_intent) {
                            Some(id) => id,
                            None => {
                                info!(target_id, "no source available (root node)");
                                continue;
                            }
                        };

                        let source_client = match clients.get(&source_id) {
                            Some(c) => Arc::clone(c),
                            None => {
                                warn!(source_id, "no client for source");
                                continue;
                            }
                        };
                        let target_client = match clients.get(target_id) {
                            Some(c) => Arc::clone(c),
                            None => {
                                warn!(target_id, "no client for target");
                                continue;
                            }
                        };

                        // Mark as InProgress
                        tracker.update_status(
                            &intent.intent_id,
                            target_id,
                            ConvergenceStatus::InProgress,
                            None,
                            None,
                        );

                        // Enumerate source digests
                        let source_set = match delta::enumerate_digests(source_client.as_ref(), &intent.repositories).await {
                            Ok(s) => s,
                            Err(e) => {
                                error!(intent_id = %intent.intent_id, source_id, "enumerate failed: {e}");
                                continue;
                            }
                        };

                        // Compute delta
                        let target_delta = match delta::compute_delta(&source_set, target_client.as_ref(), &intent.repositories).await {
                            Ok(d) => d,
                            Err(e) => {
                                error!(intent_id = %intent.intent_id, target_id, "delta failed: {e}");
                                continue;
                            }
                        };

                        if target_delta.is_empty() {
                            // Already synced — check referrer gates
                            if intent.require_referrers.is_empty() {
                                tracker.update_status(
                                    &intent.intent_id,
                                    target_id,
                                    ConvergenceStatus::Converged,
                                    None,
                                    None,
                                );
                            } else {
                                tracker.update_status(
                                    &intent.intent_id,
                                    target_id,
                                    ConvergenceStatus::ContentComplete,
                                    None,
                                    None,
                                );
                            }
                            continue;
                        }

                        // Check budget
                        if !budget_mgr.try_acquire(&source_id, target_id, target_delta.total_transfer_bytes)? {
                            tracker.add_blocker(
                                &intent.intent_id,
                                target_id,
                                peat_registry::types::ConvergenceBlockerReason::BudgetExhausted,
                                Some(format!("need {} bytes", target_delta.total_transfer_bytes)),
                            );
                            continue;
                        }

                        // Execute transfer
                        let engine = TransferEngine::new(
                            Arc::clone(&source_client),
                            Arc::clone(&target_client),
                            Arc::clone(&checkpoint_store),
                            config.transfer.clone(),
                        );

                        match engine.execute(
                            &intent.intent_id,
                            &source_id,
                            target_id,
                            &target_delta,
                            &intent.repositories,
                        ).await {
                            Ok(checkpoint) => {
                                info!(
                                    intent_id = %intent.intent_id,
                                    target_id,
                                    bytes = checkpoint.bytes_transferred,
                                    "transfer complete"
                                );

                                // Check referrer gates if required
                                if !intent.require_referrers.is_empty() {
                                    let repo = intent.repositories.first().map(|s| s.as_str()).unwrap_or("library");
                                    let mut all_passed = true;
                                    for digest in source_set.manifests.keys() {
                                        let gate = referrers::check_referrer_gates(
                                            target_client.as_ref(),
                                            repo,
                                            digest,
                                            &intent.require_referrers,
                                        ).await?;
                                        if !gate.passed {
                                            all_passed = false;
                                            break;
                                        }
                                    }

                                    if all_passed {
                                        tracker.update_status(
                                            &intent.intent_id,
                                            target_id,
                                            ConvergenceStatus::Converged,
                                            None,
                                            None,
                                        );
                                    } else {
                                        tracker.update_status(
                                            &intent.intent_id,
                                            target_id,
                                            ConvergenceStatus::ContentComplete,
                                            None,
                                            None,
                                        );
                                    }
                                } else {
                                    tracker.update_status(
                                        &intent.intent_id,
                                        target_id,
                                        ConvergenceStatus::Converged,
                                        None,
                                        None,
                                    );
                                }
                            }
                            Err(e) => {
                                error!(intent_id = %intent.intent_id, target_id, "transfer failed: {e}");
                                tracker.update_status(
                                    &intent.intent_id,
                                    target_id,
                                    ConvergenceStatus::Failed,
                                    Some(target_delta),
                                    None,
                                );
                            }
                        }
                    }

                    // Log aggregated status
                    let agg = tracker.aggregated_status(&intent.intent_id);
                    info!(
                        intent_id = %intent.intent_id,
                        total = agg.total_targets,
                        converged = agg.converged,
                        in_progress = agg.in_progress,
                        pending = agg.pending,
                        failed = agg.failed,
                        drifted = agg.drifted,
                        "sync status"
                    );
                }
            }
            _ = drift_timer.tick() => {
                for intent in &config.intents {
                    let source_id = &intent.source;
                    if let Some(source_client) = clients.get(source_id) {
                        match drift::detect_drift(
                            &intent.intent_id,
                            source_client.as_ref(),
                            &clients,
                            &intent.repositories,
                            &tracker,
                        ).await {
                            Ok(drifted) => {
                                if !drifted.is_empty() {
                                    info!(
                                        intent_id = %intent.intent_id,
                                        drifted_targets = ?drifted,
                                        "drift detected, will re-sync on next cycle"
                                    );
                                }
                            }
                            Err(e) => {
                                warn!(intent_id = %intent.intent_id, "drift detection failed: {e}");
                            }
                        }
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("shutting down");
                break;
            }
        }
    }

    info!("peat-registry stopped");
    Ok(())
}
