use std::path::PathBuf;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::util;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Metadata for a stored command output entry.
///
/// Traces the origin of stored content for debugging and filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Source system (typically "oo").
    pub source: String,

    /// Session identifier (parent process ID).
    pub session: String,

    /// The command that generated this output.
    pub command: String,

    /// Unix timestamp when this entry was created.
    pub timestamp: i64,
}

/// Result from a store search operation.
///
/// Contains the stored content along with its identifier and optional metadata.
///
/// Marked `#[non_exhaustive]` so future fields (e.g. `snippet`) can be added
/// in a non-breaking way — downstream code must use struct update syntax or
/// constructors rather than exhaustive field literals.
#[derive(Debug)]
#[non_exhaustive]
pub struct SearchResult {
    /// Unique identifier for this entry.
    pub id: String,

    /// The stored content (command output).
    pub content: String,

    /// Optional metadata about this entry's origin.
    pub meta: Option<SessionMeta>,

    /// Optional similarity score (for semantic search backends).
    #[allow(dead_code)] // Used by VipuneStore (behind feature flag)
    pub similarity: Option<f64>,

    /// Optional bounded excerpt centered on the best match.
    ///
    /// Populated by the FTS5 branch of `SqliteStore::search` via the
    /// `snippet()` FTS5 function. `None` for the short-query LIKE fallback
    /// and for backends without FTS5 (e.g. `VipuneStore`). Callers should
    /// fall back to a client-side bounded prefix of `content` when this is
    /// `None`. Note the `snippet()` window is bounded in *tokens*, so for
    /// content containing arbitrarily long multi-byte tokens a `Some` snippet
    /// can still exceed the display budget — display callers must apply their
    /// own char cap (see `recall_display::display_hit`).
    pub snippet: Option<String>,
}

// ---------------------------------------------------------------------------
// Store trait
// ---------------------------------------------------------------------------

/// Backend for storing and searching indexed command output.
///
/// Implementations can use different storage mechanisms (SQLite, Vipune, etc.)
/// to persist and retrieve command output for later recall.
pub trait Store {
    /// Index a command output entry for later retrieval.
    ///
    /// Returns the unique identifier of the indexed entry.
    fn index(
        &mut self,
        project_id: &str,
        content: &str,
        meta: &SessionMeta,
    ) -> Result<String, Error>;

    /// Search for indexed entries matching a query.
    ///
    /// Returns up to `limit` results ordered by relevance.
    fn search(
        &mut self,
        project_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, Error>;

    /// Delete all entries for a specific session.
    ///
    /// Returns the number of entries deleted.
    fn delete_by_session(&mut self, project_id: &str, session_id: &str) -> Result<usize, Error>;

    /// Delete entries older than `max_age_secs` seconds.
    ///
    /// Returns the number of entries deleted.
    fn cleanup_stale(&mut self, project_id: &str, max_age_secs: i64) -> Result<usize, Error>;
}

// ---------------------------------------------------------------------------
// SqliteStore — default backend using FTS5 for text search
// ---------------------------------------------------------------------------

/// SQLite-based store using FTS5 for full-text search.
///
/// The default backend for `oo`, indexes command output in SQLite's
/// FTS5 virtual table for efficient full-text search.
pub struct SqliteStore {
    conn: Connection,
}

fn db_path() -> PathBuf {
    // OO_DATA_DIR overrides the base directory so tests can isolate the store.
    if let Some(data_dir) = std::env::var_os("OO_DATA_DIR") {
        return PathBuf::from(data_dir).join("oo.db");
    }
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".oo")
        .join("oo.db")
}

fn map_err(e: rusqlite::Error) -> Error {
    Error::Store(e.to_string())
}

impl SqliteStore {
    /// Open the default SQLite store at `~/.local/share/.oo/oo.db`.
    ///
    /// Creates the database and tables if they don't exist.
    pub fn open() -> Result<Self, Error> {
        Self::open_at(&db_path())
    }

    /// Open a SQLite store at a specific path.
    ///
    /// Creates the database and tables if they don't exist.
    pub fn open_at(path: &std::path::Path) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Store(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(map_err)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (
                id       TEXT PRIMARY KEY,
                project  TEXT NOT NULL,
                content  TEXT NOT NULL,
                metadata TEXT,
                created  INTEGER NOT NULL
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
                content,
                content='entries',
                content_rowid='rowid'
            );
            CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
                INSERT INTO entries_fts(rowid, content)
                VALUES (new.rowid, new.content);
            END;
            CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
                INSERT INTO entries_fts(entries_fts, rowid, content)
                VALUES ('delete', old.rowid, old.content);
            END;
            CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE ON entries BEGIN
                INSERT INTO entries_fts(entries_fts, rowid, content)
                VALUES ('delete', old.rowid, old.content);
                INSERT INTO entries_fts(rowid, content)
                VALUES (new.rowid, new.content);
            END;",
        )
        .map_err(map_err)?;
        Ok(Self { conn })
    }
}

impl Store for SqliteStore {
    fn index(
        &mut self,
        project_id: &str,
        content: &str,
        meta: &SessionMeta,
    ) -> Result<String, Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let meta_json = serde_json::to_string(meta).map_err(|e| Error::Store(e.to_string()))?;
        self.conn
            .execute(
                "INSERT INTO entries (id, project, content, metadata, created)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, project_id, content, meta_json, meta.timestamp],
            )
            .map_err(map_err)?;
        Ok(id)
    }

    fn search(
        &mut self,
        project_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, Error> {
        // Use FTS5 for full-text search, fall back to LIKE if query is too short
        let results = if query.len() >= 2 {
            let mut stmt = self
                .conn
                .prepare(
                    // snippet(entries_fts, 0, '…', '…', '…', 32) — one bounded
                    // fragment centered on the best-scoring match. Column 0 is
                    // `content`. 32 tokens ≈ ~200 chars for typical ASCII output.
                    // Markers are fixed literals (not derived from user query) so
                    // there is no injection surface.
                    "SELECT e.id, e.content, e.metadata, rank,
                     snippet(entries_fts, 0, '…', '…', '…', 32)
                     FROM entries_fts f
                     JOIN entries e ON e.rowid = f.rowid
                     WHERE entries_fts MATCH ?1 AND e.project = ?2
                     ORDER BY rank
                     LIMIT ?3",
                )
                .map_err(map_err)?;

            // FTS5 query: strip embedded double-quotes before wrapping tokens to
            // prevent FTS5 syntax errors from user-supplied quotes in search terms.
            // Strip " to prevent FTS5 syntax injection. Other special chars (*, ^, -)
            // are neutralized by phrase quoting — e.g. "foo*bar" is treated as a
            // literal phrase match rather than a prefix search, which is safe and
            // correct for our use-case (exact token recall).
            let fts_query = query
                .split_whitespace()
                .map(|w| format!("\"{}\"", w.replace('"', "")))
                .collect::<Vec<_>>()
                .join(" ");

            stmt.query_map(rusqlite::params![fts_query, project_id, limit], |row| {
                let id: String = row.get(0)?;
                let content: String = row.get(1)?;
                let meta_json: Option<String> = row.get(2)?;
                let rank: f64 = row.get(3)?;
                let snippet: String = row.get(4)?;
                let snippet =
                    if snippet.is_empty() || snippet.chars().count() >= content.chars().count() {
                        None
                    } else {
                        Some(snippet)
                    };
                Ok(SearchResult {
                    id,
                    content,
                    meta: meta_json.as_deref().and_then(parse_meta),
                    similarity: Some(-rank), // FTS5 rank is negative
                    // snippet() returns the full row when the match covers the
                    // entire content (no room to trim) — map that to None so
                    // callers fall back to a client-side bounded prefix.
                    // The comparison is on `chars()` (not bytes) to stay
                    // consistent with the char-based `SNIPPET_CAP` in
                    // `recall_display`: for ASCII both are equal, and for
                    // multi-byte content the snippet is a verbatim substring of
                    // `content`, so `chars()` cannot misclassify either direction.
                    // It does NOT guarantee the snippet fits the display budget
                    // (the token-bounded window can hold one arbitrarily long
                    // multi-byte token) — that is bounded display-side.
                    snippet,
                })
            })
            .map_err(map_err)?
            .filter_map(|r| r.ok())
            .collect()
        } else {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT id, content, metadata
                     FROM entries
                     WHERE project = ?1 AND content LIKE ?2
                     ORDER BY created DESC
                     LIMIT ?3",
                )
                .map_err(map_err)?;

            let like = format!("%{query}%");
            stmt.query_map(rusqlite::params![project_id, like, limit], |row| {
                let id: String = row.get(0)?;
                let content: String = row.get(1)?;
                let meta_json: Option<String> = row.get(2)?;
                Ok(SearchResult {
                    id,
                    content,
                    meta: meta_json.as_deref().and_then(parse_meta),
                    similarity: None,
                    // No FTS5 match context in the LIKE branch — callers must
                    // fall back to a client-side bounded prefix of `content`.
                    snippet: None,
                })
            })
            .map_err(map_err)?
            .filter_map(|r| match r {
                Ok(r) => Some(r),
                // Never silently drop a matched row — surface the deserialisation
                // failure so callers can tell "fewer hits" from "no match".
                Err(e) => {
                    eprintln!("oo: warning: dropped a search result row (row error): {e}");
                    None
                }
            })
            .collect()
        };

        Ok(results)
    }

    fn delete_by_session(&mut self, project_id: &str, session_id: &str) -> Result<usize, Error> {
        // Find entries matching this session
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, metadata FROM entries WHERE project = ?1")
                .map_err(map_err)?;
            stmt.query_map(rusqlite::params![project_id], |row| {
                let id: String = row.get(0)?;
                let meta_json: Option<String> = row.get(1)?;
                Ok((id, meta_json))
            })
            .map_err(map_err)?
            .filter_map(|r| r.ok())
            .filter(|(_, meta_json)| {
                meta_json
                    .as_deref()
                    .and_then(parse_meta)
                    .is_some_and(|m| m.source == "oo" && m.session == session_id)
            })
            .map(|(id, _)| id)
            .collect()
        };

        let count = ids.len();
        for id in &ids {
            self.conn
                .execute("DELETE FROM entries WHERE id = ?1", rusqlite::params![id])
                .map_err(map_err)?;
        }
        Ok(count)
    }

    fn cleanup_stale(&mut self, project_id: &str, max_age_secs: i64) -> Result<usize, Error> {
        let now = util::now_epoch();
        let ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, metadata FROM entries WHERE project = ?1")
                .map_err(map_err)?;
            stmt.query_map(rusqlite::params![project_id], |row| {
                let id: String = row.get(0)?;
                let meta_json: Option<String> = row.get(1)?;
                Ok((id, meta_json))
            })
            .map_err(map_err)?
            .filter_map(|r| r.ok())
            .filter(|(_, meta_json)| {
                meta_json
                    .as_deref()
                    .and_then(parse_meta)
                    .is_some_and(|m| m.source == "oo" && (now - m.timestamp) > max_age_secs)
            })
            .map(|(id, _)| id)
            .collect()
        };

        let count = ids.len();
        for id in &ids {
            self.conn
                .execute("DELETE FROM entries WHERE id = ?1", rusqlite::params![id])
                .map_err(map_err)?;
        }
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// VipuneStore — optional backend with semantic search
// ---------------------------------------------------------------------------

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
        let store =
            vipune::MemoryStore::new(&config.database_path, &config.embedding_model, config)
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

    fn delete_by_session(&mut self, project_id: &str, session_id: &str) -> Result<usize, Error> {
        let entries = self
            .store
            .list(project_id, 10_000)
            .map_err(|e| Error::Store(e.to_string()))?;
        let mut count = 0;
        for entry in entries {
            if let Some(meta) = entry.metadata.as_deref().and_then(parse_meta) {
                if meta.source == "oo" && meta.session == session_id {
                    self.store
                        .delete(&entry.id)
                        .map_err(|e| Error::Store(e.to_string()))?;
                    count += 1;
                }
            }
        }
        Ok(count)
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_meta(json: &str) -> Option<SessionMeta> {
    serde_json::from_str(json).ok()
}

/// Open the default store (SqliteStore, or VipuneStore if feature-enabled).
pub fn open() -> Result<Box<dyn Store>, Error> {
    #[cfg(feature = "vipune-store")]
    {
        return Ok(Box::new(VipuneStore::open()?));
    }
    #[cfg(not(feature = "vipune-store"))]
    {
        Ok(Box::new(SqliteStore::open()?))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
