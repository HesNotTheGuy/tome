//! SQLite schema for the article store, with a versioned migration runner.

use rusqlite::{Connection, params};
use tome_core::{Result, TomeError};

const MIGRATION_1: &str = r#"
CREATE TABLE IF NOT EXISTS articles (
    page_id        INTEGER PRIMARY KEY,
    title          TEXT    NOT NULL UNIQUE,
    tier           TEXT    NOT NULL CHECK (tier IN ('hot','warm','cold','evicted')),
    pinned         INTEGER NOT NULL DEFAULT 0,
    stream_offset  INTEGER,
    stream_length  INTEGER,
    revision_id    INTEGER,
    last_accessed  INTEGER NOT NULL DEFAULT 0,
    access_count   INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_articles_tier ON articles(tier);
CREATE INDEX IF NOT EXISTS idx_articles_lru  ON articles(last_accessed);

CREATE TABLE IF NOT EXISTS hot_content (
    page_id  INTEGER PRIMARY KEY REFERENCES articles(page_id) ON DELETE CASCADE,
    wikitext TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS warm_content (
    page_id        INTEGER PRIMARY KEY REFERENCES articles(page_id) ON DELETE CASCADE,
    wikitext_zstd  BLOB    NOT NULL
);
"#;

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY);")
        .map_err(|e| TomeError::Storage(format!("create version table: {e}")))?;

    let current: Option<i32> = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
            row.get(0)
        })
        .map_err(|e| TomeError::Storage(format!("read version: {e}")))?;
    let from = current.unwrap_or(0);

    if from < 1 {
        conn.execute_batch(MIGRATION_1)
            .map_err(|e| TomeError::Storage(format!("apply migration 1: {e}")))?;
        conn.execute(
            "INSERT INTO schema_version(version) VALUES (?1)",
            params![1_i32],
        )
        .map_err(|e| TomeError::Storage(format!("record migration 1: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    #[test]
    fn fresh_db_reaches_version_1() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let v: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn tier_check_constraint_rejects_unknown_value() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let err = conn.execute(
            "INSERT INTO articles
                (page_id, title, tier, created_at, updated_at)
             VALUES (1, 'X', 'lukewarm', 0, 0)",
            [],
        );
        assert!(err.is_err());
    }
}
