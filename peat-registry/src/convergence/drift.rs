use std::collections::HashMap;
use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::convergence::state::ConvergenceTracker;
use crate::delta;
use crate::error::Result;
use crate::oci::client::RegistryClient;
use crate::types::ConvergenceStatus;

/// Performs drift detection by re-diffing source against converged targets.
///
/// If a previously converged target is now missing content that the source has,
/// transitions it to Drifted status.
pub async fn detect_drift(
    intent_id: &str,
    source_client: &dyn RegistryClient,
    target_clients: &HashMap<String, Arc<dyn RegistryClient>>,
    repositories: &[String],
    tracker: &ConvergenceTracker,
) -> Result<Vec<String>> {
    let states = tracker.get_states_for_intent(intent_id);
    let mut drifted_targets = Vec::new();

    // Only check targets that are currently converged
    let converged: Vec<_> = states
        .iter()
        .filter(|(_, s)| s.status == ConvergenceStatus::Converged)
        .collect();

    if converged.is_empty() {
        return Ok(drifted_targets);
    }

    // Enumerate source digests
    let source_set = delta::enumerate_digests(source_client, repositories).await?;

    for (target_id, _state) in converged {
        let target_client = match target_clients.get(target_id) {
            Some(c) => c,
            None => {
                warn!(
                    target_id,
                    "no client for converged target, skipping drift check"
                );
                continue;
            }
        };

        let target_delta =
            delta::compute_delta(&source_set, target_client.as_ref(), repositories).await?;

        if !target_delta.is_empty() {
            info!(
                intent_id,
                target_id,
                missing_blobs = target_delta.missing_blobs.len(),
                missing_manifests = target_delta.missing_manifests.len(),
                "drift detected"
            );
            tracker.update_status(
                intent_id,
                target_id,
                ConvergenceStatus::Drifted,
                Some(target_delta),
                None,
            );
            drifted_targets.push(target_id.clone());
        } else {
            debug!(intent_id, target_id, "no drift detected");
        }
    }

    Ok(drifted_targets)
}
