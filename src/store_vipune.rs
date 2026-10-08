//! Vipune-backed store — optional backend with semantic search.
//!
//! Uses Vipune's cross-session memory with semantic embedding search.
//! Available behind the `vipune-store` feature flag.

use crate::error::Error;
use crate::store::{SearchResult, SessionMeta, Store, parse_meta};
use crate::util;

/// Vipune-backed store with semantic search capabilities.
///
/// Uses Vipune's cross-session memory with semantic embedding search.
/// Available behind the `vipune-store` feature flag.
#[cfg(feature = "vipune-store")]
pub struct VipuneStore {
    store: vipune::MemoryStore,
}

#[cfg(feature = "vipune-store")]
impl VipuneStore {
    /// Open the Vipune store with default configuration.
    ///
    /// Loads Vipune configuration from its usual location and initializes
    /// the memory store with semantic search.
    pub fn open() -> Result<Self, Error> {
        let config = vipune::Config::load().map_err(|e| Error::Store(e.to_string()))?;
        let database_path = config.database_path.clone();
        let embedding_model = config.embedding_model.clone();
        let store = vipune::MemoryStore::new(&database_path, &embedding_model, config)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(Self { store })
    }
}

#[cfg(feature = "vipune-store")]
impl Store for VipuneStore {
    fn index(
        &mut self,
        project_id: &str,
        content: &str,
        meta: &SessionMeta,
    ) -> Result<String, Error> {
        let meta_json = serde_json::to_string(meta).map_err(|e| Error::Store(e.to_string()))?;
        match self
            .store
            .add_with_conflict(project_id, content, Some(&meta_json), true)
        {
            Ok(vipune::AddResult::Added { id }) => Ok(id),
            Ok(vipune::AddResult::Conflicts { .. }) => Ok(String::new()),
            Err(e) => Err(Error::Store(e.to_string())),
        }
    }

    fn search(
        &mut self,
        project_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, Error> {
        let memories = self
            .store
            .search_hybrid(project_id, query, limit, 0.3)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(memories
            .into_iter()
            .map(|m| SearchResult {
                id: m.id,
                meta: m.metadata.as_deref().and_then(parse_meta),
                content: m.content,
                similarity: m.similarity,
                // VipuneStore has no FTS5 — callers fall back to a bounded
                // client-side prefix of `content`.
                snippet: None,
            })
            .collect())
    }

    fn delete_project(&mut self, project_id: &str) -> Result<usize, Error> {
        // Project-scoped delete: drop every entry for this project regardless
        // of metadata, so it removes exactly what `search` can return.
        //
        // The vipune API has no bulk-delete, so we enumerate via `list` and
        // delete one-by-one. Because `list` is capped at 10 000 entries per
        // call, we loop until `list` returns a partial page (fewer than the
        // limit) to guarantee every entry is deleted, not just the first
        // 10 000.
        const PAGE: usize = 10_000;
        let mut deleted = 0;
        let mut failed = 0usize;
        let mut total = 0usize;
        loop {
            let entries = self
                .store
                .list(project_id, PAGE)
                .map_err(|e| Error::Store(e.to_string()))?;
            total += entries.len();
            for entry in &entries {
                match self.store.delete(&entry.id) {
                    // `delete` returns `Ok(true)` on success and `Ok(false)`
                    // when the id did not match any entry; both are acceptable
                    // outcomes for our purposes (the target set is the same).
                    Ok(_) => deleted += 1,
                    Err(e) => {
                        // Keep going so one bad entry does not leave the rest
                        // of the project behind; report partial progress at the end.
                        eprintln!("oo: warning: failed to delete entry {}: {e}", entry.id);
                        failed += 1;
                    }
                }
            }
            // A short page means we've drained the project; stop.
            if entries.len() < PAGE {
                break;
            }
        }
        if failed > 0 {
            return Err(Error::Store(format!(
                "partial delete: removed {deleted} of {total} entries, {failed} deletion(s) failed"
            )));
        }
        Ok(deleted)
    }

    fn cleanup_stale(&mut self, project_id: &str, max_age_secs: i64) -> Result<usize, Error> {
        let now = util::now_epoch();
        let entries = self
            .store
            .list(project_id, 10_000)
            .map_err(|e| Error::Store(e.to_string()))?;
        let mut count = 0;
        for entry in entries {
            if let Some(meta) = entry.metadata.as_deref().and_then(parse_meta) {
                if meta.source == "oo" && (now - meta.timestamp) > max_age_secs {
                    self.store
                        .delete(&entry.id)
                        .map_err(|e| Error::Store(e.to_string()))?;
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}
