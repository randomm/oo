use std::path::PathBuf;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::Error;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub source: String,
    pub session: String,
    pub command: String,
    pub timestamp: i64,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub meta: Option<SessionMeta>,
    pub similarity: Option<f64>,
}

// ---------------------------------------------------------------------------
// Store trait
// ---------------------------------------------------------------------------

pub trait Store {
    fn index(
        &mut self,
        project_id: &str,
        content: &str,
        meta: &SessionMeta,
    ) -> Result<String, Error>;

    fn search(
        &mut self,
        project_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, Error>;

    fn delete_by_session(&mut self, project_id: &str, session_id: &str) -> Result<usize, Error>;

    fn cleanup_stale(&mut self, project_id: &str, max_age_secs: i64) -> Result<usize, Error>;
}

// ---------------------------------------------------------------------------
// SqliteStore — default backend using FTS5 for text search
// ---------------------------------------------------------------------------

pub struct SqliteStore {
    conn: Connection,
}

fn db_path() -> PathBuf {
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
    pub fn open() -> Result<Self, Error> {
        Self::open_at(&db_path())
    }

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
                    "SELECT e.id, e.content, e.metadata, rank
                     FROM entries_fts f
                     JOIN entries e ON e.rowid = f.rowid
                     WHERE entries_fts MATCH ?1 AND e.project = ?2
                     ORDER BY rank
                     LIMIT ?3",
                )
                .map_err(map_err)?;

            // FTS5 query: wrap tokens for prefix matching
            let fts_query = query
                .split_whitespace()
                .map(|w| format!("\"{w}\""))
                .collect::<Vec<_>>()
                .join(" ");

            stmt.query_map(rusqlite::params![fts_query, project_id, limit], |row| {
                let id: String = row.get(0)?;
                let content: String = row.get(1)?;
                let meta_json: Option<String> = row.get(2)?;
                let rank: f64 = row.get(3)?;
                Ok(SearchResult {
                    id,
                    content,
                    meta: meta_json.as_deref().and_then(parse_meta),
                    similarity: Some(-rank), // FTS5 rank is negative
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
                })
            })
            .map_err(map_err)?
            .filter_map(|r| r.ok())
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
        let now = now_epoch();
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

#[cfg(feature = "vipune-store")]
pub struct VipuneStore {
    store: vipune::MemoryStore,
}

#[cfg(feature = "vipune-store")]
impl VipuneStore {
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
        let now = now_epoch();
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

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
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
mod tests {
    use super::*;

    fn test_meta(session: &str) -> SessionMeta {
        SessionMeta {
            source: "oo".into(),
            session: session.into(),
            command: "test cmd".into(),
            timestamp: now_epoch(),
        }
    }

    fn temp_store() -> SqliteStore {
        SqliteStore::open_at(
            &std::env::temp_dir().join(format!("oo-test-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap()
    }

    #[test]
    fn test_index_and_search() {
        let mut store = temp_store();
        let meta = test_meta("s1");
        store
            .index("proj", "auth bug in login flow", &meta)
            .unwrap();
        store
            .index("proj", "database migration issue", &meta)
            .unwrap();

        let results = store.search("proj", "auth", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("auth"));
    }

    #[test]
    fn test_search_no_results() {
        let mut store = temp_store();
        let results = store.search("proj", "nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_delete_session() {
        let mut store = temp_store();
        store.index("proj", "a", &test_meta("s1")).unwrap();
        store.index("proj", "b", &test_meta("s2")).unwrap();
        store.index("proj", "c", &test_meta("s1")).unwrap();

        let deleted = store.delete_by_session("proj", "s1").unwrap();
        assert_eq!(deleted, 2);

        let remaining = store.search("proj", "b", 10).unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn test_cleanup_stale() {
        let mut store = temp_store();
        let old_meta = SessionMeta {
            source: "oo".into(),
            session: "s1".into(),
            command: "old".into(),
            timestamp: now_epoch() - 100_000,
        };
        let fresh_meta = test_meta("s1");

        store.index("proj", "old data here", &old_meta).unwrap();
        store.index("proj", "fresh data here", &fresh_meta).unwrap();

        let deleted = store.cleanup_stale("proj", 86400).unwrap();
        assert_eq!(deleted, 1);

        // Only fresh remains
        let results = store.search("proj", "data", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("fresh"));
    }

    #[test]
    fn test_metadata_round_trip() {
        let mut store = temp_store();
        let meta = test_meta("sess123");
        store
            .index("proj", "test content for round trip", &meta)
            .unwrap();

        let results = store.search("proj", "round trip", 10).unwrap();
        assert_eq!(results.len(), 1);
        let found_meta = results[0].meta.as_ref().unwrap();
        assert_eq!(found_meta.source, "oo");
        assert_eq!(found_meta.session, "sess123");
        assert_eq!(found_meta.command, "test cmd");
    }
}
