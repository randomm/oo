//! Vipune-backed store — optional backend with semantic search.
//!
//! Uses Vipune's cross-session memory with semantic embedding search.
//! Available behind the `vipune-store` feature flag.

use crate::error::Error;
use crate::project_drain::{
    self, Entry, EntryId, LIST_CAP, MAX_PASSES, ProjectEntries, partial_delete_error,
};
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

/// `Store` for Vipune.
///
/// `delete_project` and `cleanup_stale` run the shared loops in
/// `crate::project_drain`, which list and delete page by page. Vipune has no
/// bulk delete and no offset, so a project is drained only when a listing is
/// empty; any other stop is an error. Stale cleanup inspects one window of the
/// newest entries Vipune lists and reports failed deletes as an error.
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
        // of metadata.
        let drain = ProjectDrain {
            store: &self.store,
            project_id,
        };
        project_drain::drain_project(&drain, LIST_CAP, MAX_PASSES)
    }

    fn cleanup_stale(&mut self, project_id: &str, max_age_secs: i64) -> Result<usize, Error> {
        let cutoff = project_drain::stale_cutoff(util::now_epoch(), max_age_secs);
        let drain = ProjectDrain {
            store: &self.store,
            project_id,
        };
        let out = project_drain::cleanup_stale_in(&drain, cutoff, LIST_CAP)?;
        if out.window_capped {
            project_drain::warn_window_capped_once(&mut std::io::stderr(), &WINDOW_WARNED);
        }
        if out.failed > 0 {
            return Err(partial_delete_error(out.deleted, out.failed));
        }
        Ok(out.deleted)
    }
}

/// Process-wide: both the retention purge and `try_index` call
/// `cleanup_stale`, so the window-capped warning must not repeat within one
/// process. Deliberately not per store instance.
static WINDOW_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Adapter scope for the drain functions: borrows the Vipune store and pins
/// the project id, so the backend-agnostic loops never need to know either.
struct ProjectDrain<'a> {
    store: &'a vipune::MemoryStore,
    project_id: &'a str,
}

/// `ProjectEntries` adapter over a `VipuneStore`. It maps Vipune rows and
/// leaves the staleness decision to `crate::project_drain`. Enumeration uses
/// `list`, never `search`: search is similarity-based and would not return
/// every entry.
impl ProjectEntries for ProjectDrain<'_> {
    fn list(&self, limit: usize) -> Result<Vec<Entry>, Error> {
        let memories = self
            .store
            .list(self.project_id, limit)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(memories
            .into_iter()
            .map(|m| {
                let meta = m.metadata.as_deref().and_then(parse_meta);
                let is_oo_entry = meta.as_ref().is_some_and(|meta| meta.source == "oo");
                Entry {
                    id: m.id,
                    timestamp: meta.map_or(0, |meta| meta.timestamp),
                    is_oo_entry,
                }
            })
            .collect())
    }

    fn list_ids(&self, limit: usize) -> Result<Vec<EntryId>, Error> {
        let memories = self
            .store
            .list(self.project_id, limit)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(memories.into_iter().map(|m| m.id).collect())
    }

    fn delete(&self, id: &str) -> Result<bool, Error> {
        self.store
            .delete(id)
            .map_err(|e| Error::Store(e.to_string()))
    }
}
