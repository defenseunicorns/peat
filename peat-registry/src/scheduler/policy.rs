use crate::types::{DdilPolicyClass, SyncPriority};

impl DdilPolicyClass {
    /// Order intents by policy class (MissionCritical > MissionSupport > Background).
    pub fn priority_order(&self) -> u32 {
        match self {
            DdilPolicyClass::MissionCritical => 3,
            DdilPolicyClass::MissionSupport => 2,
            DdilPolicyClass::Background => 1,
        }
    }
}

/// Compare two intents for scheduling order. Higher result = schedule first.
pub fn scheduling_key(policy: &DdilPolicyClass, priority: &SyncPriority) -> u64 {
    let class_weight = policy.priority_order() as u64 * 100;
    let priority_weight = *priority as u64;
    class_weight + priority_weight
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduling_order() {
        let mc_crit = scheduling_key(&DdilPolicyClass::MissionCritical, &SyncPriority::Critical);
        let mc_low = scheduling_key(&DdilPolicyClass::MissionCritical, &SyncPriority::Low);
        let ms_crit = scheduling_key(&DdilPolicyClass::MissionSupport, &SyncPriority::Critical);
        let bg_crit = scheduling_key(&DdilPolicyClass::Background, &SyncPriority::Critical);

        assert!(mc_crit > mc_low);
        assert!(mc_low > ms_crit);
        assert!(ms_crit > bg_crit);
    }

    #[test]
    fn test_policy_params() {
        let mc = DdilPolicyClass::MissionCritical.params();
        let ms = DdilPolicyClass::MissionSupport.params();
        let bg = DdilPolicyClass::Background.params();

        assert!(mc.max_concurrency > ms.max_concurrency);
        assert!(ms.max_concurrency > bg.max_concurrency);
        assert!(!mc.preemptable);
        assert!(!ms.preemptable);
        assert!(bg.preemptable);
    }
}
