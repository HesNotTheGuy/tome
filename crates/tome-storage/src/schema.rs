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

const MIGRATION_2: &str = r#"
CREATE TABLE IF NOT EXISTS geotags (
    page_id  INTEGER NOT NULL,
    lat      REAL    NOT NULL,
    lon      REAL    NOT NULL,
    primary_ INTEGER NOT NULL DEFAULT 0,
    kind     TEXT,
    PRIMARY KEY (page_id, lat, lon)
);

CREATE INDEX IF NOT EXISTS idx_geotags_page ON geotags(page_id);
"#;

const MIGRATION_3: &str = r#"
CREATE TABLE IF NOT EXISTS categorylinks (
    cl_from   INTEGER NOT NULL,
    cl_to     TEXT    NOT NULL,
    cl_type   TEXT    NOT NULL CHECK (cl_type IN ('page','subcat','file')),
    PRIMARY KEY (cl_from, cl_to)
);

CREATE INDEX IF NOT EXISTS idx_categorylinks_to ON categorylinks(cl_to);
CREATE INDEX IF NOT EXISTS idx_categorylinks_from ON categorylinks(cl_from);
"#;

const MIGRATION_4: &str = r#"
CREATE TABLE IF NOT EXISTS redirects (
    from_page_id  INTEGER PRIMARY KEY,
    target_title  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_redirects_target ON redirects(target_title);
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

    if from < 2 {
        conn.execute_batch(MIGRATION_2)
            .map_err(|e| TomeError::Storage(format!("apply migration 2: {e}")))?;
        conn.execute(
            "INSERT INTO schema_version(version) VALUES (?1)",
            params![2_i32],
        )
        .map_err(|e| TomeError::Storage(format!("record migration 2: {e}")))?;
    }

    if from < 3 {
        conn.execute_batch(MIGRATION_3)
            .map_err(|e| TomeError::Storage(format!("apply migration 3: {e}")))?;
        conn.execute(
            "INSERT INTO schema_version(version) VALUES (?1)",
            params![3_i32],
        )
        .map_err(|e| TomeError::Storage(format!("record migration 3: {e}")))?;
    }

    if from < 4 {
        conn.execute_batch(MIGRATION_4)
            .map_err(|e| TomeError::Storage(format!("apply migration 4: {e}")))?;
        conn.execute(
            "INSERT INTO schema_version(version) VALUES (?1)",
            params![4_i32],
        )
        .map_err(|e| TomeError::Storage(format!("record migration 4: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;

    /// The highest migration version this codebase ships. Bump this in lockstep
    /// with new MIGRATION_N constants; the assertion below will catch
    /// mismatches.
    const CURRENT_VERSION: i32 = 4;

    #[test]
    fn fresh_db_reaches_current_version() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let v: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(v, CURRENT_VERSION);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let first: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(first, after, "re-running migrate must not add rows");
    }

    #[test]
    fn geotags_table_present_after_migrate() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='geotags'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "geotags table missing after migrate");
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
