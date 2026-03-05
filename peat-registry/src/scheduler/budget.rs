use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use tracing::debug;

use crate::error::Result;

/// Tracks bandwidth consumption per edge with a windowed budget (bytes/hour).
struct EdgeBudget {
    budget_bytes_per_hour: u64,
    consumed_bytes: u64,
    window_start: DateTime<Utc>,
}

impl EdgeBudget {
    fn new(budget: u64) -> Self {
        Self {
            budget_bytes_per_hour: budget,
            consumed_bytes: 0,
            window_start: Utc::now(),
        }
    }

    fn reset_if_window_expired(&mut self) {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(self.window_start).num_seconds();
        if elapsed >= 3600 {
            self.consumed_bytes = 0;
            self.window_start = now;
        }
    }

    fn remaining(&mut self) -> u64 {
        self.reset_if_window_expired();
        self.budget_bytes_per_hour
            .saturating_sub(self.consumed_bytes)
    }

    fn try_acquire(&mut self, bytes: u64) -> bool {
        self.reset_if_window_expired();
        if self.consumed_bytes + bytes <= self.budget_bytes_per_hour {
            self.consumed_bytes += bytes;
            true
        } else {
            false
        }
    }

    fn release(&mut self, bytes: u64) {
        self.consumed_bytes = self.consumed_bytes.saturating_sub(bytes);
    }
}

/// Manages bandwidth budgets across all edges in the topology.
pub struct BudgetManager {
    /// Edge key "source_id->target_id" → budget tracker
    budgets: Mutex<HashMap<String, EdgeBudget>>,
}

impl BudgetManager {
    pub fn new() -> Self {
        Self {
            budgets: Mutex::new(HashMap::new()),
        }
    }

    /// Register a budget for an edge.
    pub fn register_edge(&self, source_id: &str, target_id: &str, bytes_per_hour: u64) {
        let key = Self::edge_key(source_id, target_id);
        let mut budgets = self.budgets.lock().unwrap();
        budgets.insert(key, EdgeBudget::new(bytes_per_hour));
    }

    /// Try to acquire bandwidth on an edge. Returns true if budget allows.
    pub fn try_acquire(&self, source_id: &str, target_id: &str, bytes: u64) -> Result<bool> {
        let key = Self::edge_key(source_id, target_id);
        let mut budgets = self.budgets.lock().unwrap();

        match budgets.get_mut(&key) {
            Some(budget) => {
                let ok = budget.try_acquire(bytes);
                if !ok {
                    debug!(
                        source_id,
                        target_id,
                        bytes,
                        remaining = budget.remaining(),
                        "budget exhausted"
                    );
                }
                Ok(ok)
            }
            None => Ok(true), // No budget constraint — unlimited
        }
    }

    /// Release bandwidth back to the budget (e.g., on transfer failure).
    pub fn release(&self, source_id: &str, target_id: &str, bytes: u64) {
        let key = Self::edge_key(source_id, target_id);
        let mut budgets = self.budgets.lock().unwrap();
        if let Some(budget) = budgets.get_mut(&key) {
            budget.release(bytes);
        }
    }

    /// Check remaining budget on an edge.
    pub fn remaining(&self, source_id: &str, target_id: &str) -> u64 {
        let key = Self::edge_key(source_id, target_id);
        let mut budgets = self.budgets.lock().unwrap();
        match budgets.get_mut(&key) {
            Some(budget) => budget.remaining(),
            None => u64::MAX,
        }
    }

    fn edge_key(source_id: &str, target_id: &str) -> String {
        format!("{}->{}", source_id, target_id)
    }
}

impl Default for BudgetManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_acquire_release() {
        let mgr = BudgetManager::new();
        mgr.register_edge("src", "tgt", 10000);

        assert!(mgr.try_acquire("src", "tgt", 5000).unwrap());
        assert_eq!(mgr.remaining("src", "tgt"), 5000);

        assert!(mgr.try_acquire("src", "tgt", 5000).unwrap());
        assert_eq!(mgr.remaining("src", "tgt"), 0);

        // Over budget
        assert!(!mgr.try_acquire("src", "tgt", 1).unwrap());

        // Release some
        mgr.release("src", "tgt", 3000);
        assert_eq!(mgr.remaining("src", "tgt"), 3000);
        assert!(mgr.try_acquire("src", "tgt", 3000).unwrap());
    }

    #[test]
    fn test_budget_no_constraint() {
        let mgr = BudgetManager::new();
        // No edge registered — should always succeed
        assert!(mgr.try_acquire("any", "edge", 999999).unwrap());
        assert_eq!(mgr.remaining("any", "edge"), u64::MAX);
    }

    #[test]
    fn test_budget_multiple_edges() {
        let mgr = BudgetManager::new();
        mgr.register_edge("a", "b", 1000);
        mgr.register_edge("a", "c", 2000);

        assert!(mgr.try_acquire("a", "b", 800).unwrap());
        assert!(mgr.try_acquire("a", "c", 1500).unwrap());
        assert_eq!(mgr.remaining("a", "b"), 200);
        assert_eq!(mgr.remaining("a", "c"), 500);
    }
}
