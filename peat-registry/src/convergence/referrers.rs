use std::collections::HashSet;

use tracing::debug;

use crate::error::Result;
use crate::oci::client::RegistryClient;
use crate::types::ReferrerGateResult;

/// Check whether all required referrer artifact types are present for a digest.
///
/// Uses the OCI 1.1 Referrers API to enumerate referrers and check that
/// each required artifact type has at least one referrer present.
pub async fn check_referrer_gates(
    target_client: &dyn RegistryClient,
    repo: &str,
    digest: &str,
    required_types: &[String],
) -> Result<ReferrerGateResult> {
    if required_types.is_empty() {
        return Ok(ReferrerGateResult {
            passed: true,
            present_types: HashSet::new(),
            missing_types: vec![],
        });
    }

    let referrers = target_client.list_referrers(repo, digest, None).await?;

    let present_types: HashSet<String> =
        referrers.iter().map(|r| r.artifact_type.clone()).collect();

    let missing_types: Vec<String> = required_types
        .iter()
        .filter(|t| !present_types.contains(*t))
        .cloned()
        .collect();

    let passed = missing_types.is_empty();

    debug!(
        repo,
        digest,
        ?present_types,
        ?missing_types,
        passed,
        "referrer gate check"
    );

    Ok(ReferrerGateResult {
        passed,
        present_types,
        missing_types,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_referrer_gate_result_empty_requirements() {
        let result = ReferrerGateResult {
            passed: true,
            present_types: HashSet::new(),
            missing_types: vec![],
        };
        assert!(result.passed);
    }

    #[test]
    fn test_referrer_gate_result_with_missing() {
        let result = ReferrerGateResult {
            passed: false,
            present_types: HashSet::from(["application/vnd.cncf.notary.signature".to_string()]),
            missing_types: vec!["application/vnd.in-toto.provenance+json".to_string()],
        };
        assert!(!result.passed);
        assert_eq!(result.missing_types.len(), 1);
    }
}
