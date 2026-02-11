//! Re-exported from hive-mesh. See [`hive_mesh::storage::automerge_store`].
pub use hive_mesh::storage::automerge_store::*;

// === GcStore trait implementation for AutomergeStore (ADR-034 Phase 3) ===
//
// GcStore trait is defined in hive-protocol, so the impl must stay here.
// All methods delegate to inherent methods on AutomergeStore (now in hive-mesh).

impl crate::qos::GcStore for AutomergeStore {
    fn get_all_tombstones(&self) -> anyhow::Result<Vec<crate::qos::Tombstone>> {
        self.get_all_tombstones()
    }

    fn remove_tombstone(&self, collection: &str, document_id: &str) -> anyhow::Result<bool> {
        self.remove_tombstone(collection, document_id)
    }

    fn has_tombstone(&self, collection: &str, document_id: &str) -> anyhow::Result<bool> {
        self.has_tombstone(collection, document_id)
    }

    fn get_expired_documents(
        &self,
        collection: &str,
        cutoff: std::time::SystemTime,
    ) -> anyhow::Result<Vec<String>> {
        self.get_expired_documents(collection, cutoff)
    }

    fn hard_delete(&self, collection: &str, document_id: &str) -> anyhow::Result<()> {
        self.hard_delete(collection, document_id)
    }

    fn list_collections(&self) -> anyhow::Result<Vec<String>> {
        self.list_collections()
    }
}
