//! Revision archive.
//!
//! Owns the user's permanently-saved historical revisions in a SQLite
//! database separate from the main article store. Saved revisions are full
//! content, accessible offline forever, and survive dump replacement. A
//! built-in FTS5 index lets the UI search across notes and content.
//!
//! Diff between two revisions is delegated to the MediaWiki API's
//! `action=compare` and lives in `tome-services`; this module stores the
//! revisions, it does not generate diffs.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tome_core::{Result, TomeError};

const MIGRATION_1: &str = r#"
CREATE TABLE IF NOT EXISTS saved_revisions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    title         TEXT NOT NULL,
    revision_id   INTEGER NOT NULL,
    fetched_at    INTEGER NOT NULL,
    wikitext      TEXT NOT NULL,
    html          TEXT,
    user_note     TEXT,
    UNIQUE(title, revision_id)
);

CREATE INDEX IF NOT EXISTS idx_saved_revisions_title ON saved_revisions(title);
CREATE INDEX IF NOT EXISTS idx_saved_revisions_fetched_at ON saved_revisions(fetched_at);

CREATE VIRTUAL TABLE IF NOT EXISTS saved_revisions_fts USING fts5(
    title, wikitext, user_note,
    content='saved_revisions',
    content_rowid='id',
    tokenize='porter'
);

CREATE TRIGGER IF NOT EXISTS saved_revisions_ai AFTER INSERT ON saved_revisions BEGIN
    INSERT INTO saved_revisions_fts(rowid, title, wikitext, user_note)
    VALUES (new.id, new.title, new.wikitext, COALESCE(new.user_note, ''));
END;

CREATE TRIGGER IF NOT EXISTS saved_revisions_ad AFTER DELETE ON saved_revisions BEGIN
    INSERT INTO saved_revisions_fts(saved_revisions_fts, rowid, title, wikitext, user_note)
    VALUES ('delete', old.id, old.title, old.wikitext, COALESCE(old.user_note, ''));
END;

CREATE TRIGGER IF NOT EXISTS saved_revisions_au AFTER UPDATE ON saved_revisions BEGIN
    INSERT INTO saved_revisions_fts(saved_revisions_fts, rowid, title, wikitext, user_note)
    VALUES ('delete', old.id, old.title, old.wikitext, COALESCE(old.user_note, ''));
    INSERT INTO saved_revisions_fts(rowid, title, wikitext, user_note)
    VALUES (new.id, new.title, new.wikitext, COALESCE(new.user_note, ''));
END;
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedRevision {
    pub id: i64,
    pub title: String,
    pub revision_id: u64,
    pub fetched_at: i64,
    pub wikitext: String,
    pub html: Option<String>,
    pub user_note: Option<String>,
}

/// Cheap metadata-only listing — used by the archive sidebar where loading
/// the full wikitext for every saved revision would be wasteful.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedRevisionMeta {
    pub id: i64,
    pub title: String,
    pub revision_id: u64,
    pub fetched_at: i64,
    pub user_note: Option<String>,
}

pub struct ArchiveStore {
    conn: Mutex<Connection>,
}

impl ArchiveStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| TomeError::Storage(format!("open archive at {path:?}: {e}")))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| TomeError::Storage(format!("open in-memory archive: {e}")))?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| TomeError::Storage(format!("enable foreign keys: {e}")))?;
        conn.execute_batch(MIGRATION_1)
            .map_err(|e| TomeError::Storage(format!("apply schema: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| TomeError::Storage(format!("archive mutex poisoned: {e}")))
    }

    /// Save a revision permanently. Returns the local `id` of the row.
    /// If `(title, revision_id)` already exists, the existing row is updated
    /// in place — this is how a user "edits" their saved note.
    pub fn save(
        &self,
        title: &str,
        revision_id: u64,
        wikitext: &str,
        html: Option<&str>,
        user_note: Option<&str>,
    ) -> Result<i64> {
        let conn = self.lock()?;
        let now_ts = now_secs();
        conn.execute(
            "INSERT INTO saved_revisions
                (title, revision_id, fetched_at, wikitext, html, user_note)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(title, revision_id) DO UPDATE SET
                wikitext  = excluded.wikitext,
                html      = excluded.html,
                user_note = excluded.user_note",
            params![title, revision_id as i64, now_ts, wikitext, html, user_note],
        )
        .map_err(|e| TomeError::Storage(format!("save revision: {e}")))?;
        let id: i64 = conn
            .query_row(
                "SELECT id FROM saved_revisions WHERE title = ?1 AND revision_id = ?2",
                params![title, revision_id as i64],
                |row| row.get(0),
            )
            .map_err(|e| TomeError::Storage(format!("read saved id: {e}")))?;
        Ok(id)
    }

    pub fn get(&self, id: i64) -> Result<Option<SavedRevision>> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT id, title, revision_id, fetched_at, wikitext, html, user_note
             FROM saved_revisions WHERE id = ?1",
            params![id],
            |row| {
                Ok(SavedRevision {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    revision_id: row.get::<_, i64>(2)? as u64,
                    fetched_at: row.get(3)?,
                    wikitext: row.get(4)?,
                    html: row.get(5)?,
                    user_note: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| TomeError::Storage(format!("get revision: {e}")))
    }

    pub fn list(&self) -> Result<Vec<SavedRevisionMeta>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, revision_id, fetched_at, user_note
                 FROM saved_revisions ORDER BY fetched_at DESC",
            )
            .map_err(|e| TomeError::Storage(format!("prepare list: {e}")))?;
        let rows = stmt
            .query_map([], row_to_meta)
            .map_err(|e| TomeError::Storage(format!("query list: {e}")))?;
        collect(rows)
    }

    pub fn list_by_title(&self, title: &str) -> Result<Vec<SavedRevisionMeta>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, revision_id, fetched_at, user_note
                 FROM saved_revisions WHERE title = ?1 ORDER BY revision_id DESC",
            )
            .map_err(|e| TomeError::Storage(format!("prepare list_by_title: {e}")))?;
        let rows = stmt
            .query_map(params![title], row_to_meta)
            .map_err(|e| TomeError::Storage(format!("query list_by_title: {e}")))?;
        collect(rows)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let conn = self.lock()?;
        let n = conn
            .execute("DELETE FROM saved_revisions WHERE id = ?1", params![id])
            .map_err(|e| TomeError::Storage(format!("delete revision: {e}")))?;
        if n == 0 {
            return Err(TomeError::NotFound(format!("saved revision {id}")));
        }
        Ok(())
    }

    /// FTS5 search across title, wikitext, and user_note. Results ordered by
    /// rank (BM25 within the FTS5 index).
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SavedRevisionMeta>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT r.id, r.title, r.revision_id, r.fetched_at, r.user_note
                 FROM saved_revisions r
                 JOIN saved_revisions_fts f ON f.rowid = r.id
                 WHERE saved_revisions_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )
            .map_err(|e| TomeError::Storage(format!("prepare search: {e}")))?;
        let rows = stmt
            .query_map(params![query, limit as i64], row_to_meta)
            .map_err(|e| TomeError::Storage(format!("execute search: {e}")))?;
        collect(rows)
    }

    pub fn count(&self) -> Result<u64> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM saved_revisions", [], |row| row.get(0))
            .map_err(|e| TomeError::Storage(format!("count revisions: {e}")))?;
        Ok(n as u64)
    }
}

fn row_to_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedRevisionMeta> {
    Ok(SavedRevisionMeta {
        id: row.get(0)?,
        title: row.get(1)?,
        revision_id: row.get::<_, i64>(2)? as u64,
        fetched_at: row.get(3)?,
        user_note: row.get(4)?,
    })
}

fn collect(
    rows: rusqlite::MappedRows<
        '_,
        impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<SavedRevisionMeta>,
    >,
) -> Result<Vec<SavedRevisionMeta>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| TomeError::Storage(format!("row: {e}")))?);
    }
    Ok(out)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_get() {
        let store = ArchiveStore::open_in_memory().unwrap();
        let id = store
            .save(
                "Photon",
                123,
                "A photon is...",
                Some("<p>A photon is...</p>"),
                Some("Saved for class notes"),
            )
            .unwrap();
        let got = store.get(id).unwrap().unwrap();
        assert_eq!(got.title, "Photon");
        assert_eq!(got.revision_id, 123);
        assert_eq!(got.wikitext, "A photon is...");
        assert_eq!(got.html.as_deref(), Some("<p>A photon is...</p>"));
        assert_eq!(got.user_note.as_deref(), Some("Saved for class notes"));
    }

    #[test]
    fn save_same_title_revision_updates_in_place() {
        let store = ArchiveStore::open_in_memory().unwrap();
        let id1 = store.save("Photon", 123, "first", None, None).unwrap();
        let id2 = store
            .save("Photon", 123, "second", None, Some("note"))
            .unwrap();
        assert_eq!(id1, id2, "same (title, rev) should reuse the row");
        let got = store.get(id1).unwrap().unwrap();
        assert_eq!(got.wikitext, "second");
        assert_eq!(got.user_note.as_deref(), Some("note"));
    }

    #[test]
    fn list_orders_newest_first() {
        let store = ArchiveStore::open_in_memory().unwrap();
        let _id1 = store.save("A", 1, "a", None, None).unwrap();
        let _id2 = store.save("B", 1, "b", None, None).unwrap();
        let _id3 = store.save("C", 1, "c", None, None).unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 3);
        // Last-saved is first (DESC order). Wall-clock-second resolution may
        // mean ties; with monotonic insertion ids the last id should still be
        // newest-first when timestamps tie.
        let titles: Vec<_> = listed.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, vec!["C", "B", "A"]);
    }

    #[test]
    fn list_by_title_returns_all_revisions_for_one_article() {
        let store = ArchiveStore::open_in_memory().unwrap();
        store.save("Photon", 100, "v1", None, None).unwrap();
        store.save("Photon", 200, "v2", None, None).unwrap();
        store.save("Electron", 50, "e", None, None).unwrap();
        let photons = store.list_by_title("Photon").unwrap();
        assert_eq!(photons.len(), 2);
        // Sorted by revision_id DESC.
        assert_eq!(photons[0].revision_id, 200);
        assert_eq!(photons[1].revision_id, 100);
    }

    #[test]
    fn delete_removes_row_and_unknown_id_errors() {
        let store = ArchiveStore::open_in_memory().unwrap();
        let id = store.save("X", 1, "body", None, None).unwrap();
        store.delete(id).unwrap();
        assert!(store.get(id).unwrap().is_none());
        let err = store.delete(id).unwrap_err();
        assert!(matches!(err, TomeError::NotFound(_)));
    }

    #[test]
    fn search_matches_wikitext() {
        let store = ArchiveStore::open_in_memory().unwrap();
        store
            .save(
                "Photon",
                1,
                "A photon is an elementary particle.",
                None,
                None,
            )
            .unwrap();
        store
            .save("Cooking", 1, "Cooking involves heat.", None, None)
            .unwrap();
        let hits = store.search("particle", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Photon");
    }

    #[test]
    fn search_matches_user_note() {
        let store = ArchiveStore::open_in_memory().unwrap();
        store
            .save(
                "Photon",
                1,
                "irrelevant body",
                None,
                Some("class notes for physics"),
            )
            .unwrap();
        store
            .save("Cooking", 1, "irrelevant body", None, Some("Sunday dinner"))
            .unwrap();
        let hits = store.search("physics", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Photon");
    }

    #[test]
    fn search_matches_title() {
        let store = ArchiveStore::open_in_memory().unwrap();
        store.save("Photon", 1, "x", None, None).unwrap();
        store.save("Electron", 1, "y", None, None).unwrap();
        let hits = store.search("electron", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Electron");
    }

    #[test]
    fn delete_keeps_fts_consistent() {
        let store = ArchiveStore::open_in_memory().unwrap();
        let id = store.save("Photon", 1, "particle", None, None).unwrap();
        assert_eq!(store.search("particle", 10).unwrap().len(), 1);
        store.delete(id).unwrap();
        assert_eq!(store.search("particle", 10).unwrap().len(), 0);
    }

    #[test]
    fn count_reflects_inserts_and_deletes() {
        let store = ArchiveStore::open_in_memory().unwrap();
        assert_eq!(store.count().unwrap(), 0);
        let id = store.save("A", 1, "a", None, None).unwrap();
        store.save("B", 1, "b", None, None).unwrap();
        assert_eq!(store.count().unwrap(), 2);
        store.delete(id).unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }
}
