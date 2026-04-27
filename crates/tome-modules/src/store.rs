//! Persistent tracker of installed modules.
//!
//! SQLite-backed and intentionally separate from the article store: a module
//! is a pointer to a set of titles, the article store owns content and tier.
//! The two stores are joined at the services layer.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tome_core::{Result, TomeError};

use crate::spec::ModuleSpec;

const MIGRATION_1: &str = r#"
CREATE TABLE IF NOT EXISTS modules (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    default_tier  TEXT NOT NULL,
    spec_json     TEXT NOT NULL,
    installed_at  INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS module_members (
    module_id  TEXT NOT NULL REFERENCES modules(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    PRIMARY KEY (module_id, title)
);

CREATE INDEX IF NOT EXISTS idx_module_members_title ON module_members(title);
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstalledModule {
    pub spec: ModuleSpec,
    pub member_count: u64,
    pub installed_at: i64,
    pub updated_at: i64,
}

pub struct ModuleStore {
    conn: Mutex<Connection>,
}

impl ModuleStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| TomeError::Storage(format!("open module store at {path:?}: {e}")))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| TomeError::Storage(format!("open in-memory module store: {e}")))?;
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
            .map_err(|e| TomeError::Storage(format!("module store mutex poisoned: {e}")))
    }

    /// Install or replace a module along with its resolved member titles.
    /// `members` should already include both explicit_titles and any
    /// category-derived titles; this layer does not call any resolver.
    pub fn install(&self, spec: &ModuleSpec, members: &[String]) -> Result<()> {
        spec.validate()?;
        let spec_json = serde_json::to_string(spec)
            .map_err(|e| TomeError::Storage(format!("serialize spec: {e}")))?;
        let mut conn = self.lock()?;
        let now_ts = now_secs();
        let tx = conn
            .transaction()
            .map_err(|e| TomeError::Storage(format!("begin tx: {e}")))?;

        tx.execute(
            "INSERT INTO modules
                (id, name, description, default_tier, spec_json, installed_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
                name         = excluded.name,
                description  = excluded.description,
                default_tier = excluded.default_tier,
                spec_json    = excluded.spec_json,
                updated_at   = excluded.updated_at",
            params![
                spec.id,
                spec.name,
                spec.description,
                spec.default_tier.as_str(),
                spec_json,
                now_ts,
            ],
        )
        .map_err(|e| TomeError::Storage(format!("upsert module: {e}")))?;

        // Replace member set: cascade-delete old, insert new.
        tx.execute(
            "DELETE FROM module_members WHERE module_id = ?1",
            params![spec.id],
        )
        .map_err(|e| TomeError::Storage(format!("clear members: {e}")))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO module_members (module_id, title) VALUES (?1, ?2)
                     ON CONFLICT DO NOTHING",
                )
                .map_err(|e| TomeError::Storage(format!("prepare insert member: {e}")))?;
            for title in members {
                stmt.execute(params![spec.id, title])
                    .map_err(|e| TomeError::Storage(format!("insert member: {e}")))?;
            }
        }

        tx.commit()
            .map_err(|e| TomeError::Storage(format!("commit install: {e}")))?;
        Ok(())
    }

    pub fn uninstall(&self, id: &str) -> Result<()> {
        let conn = self.lock()?;
        let n = conn
            .execute("DELETE FROM modules WHERE id = ?1", params![id])
            .map_err(|e| TomeError::Storage(format!("delete module: {e}")))?;
        if n == 0 {
            return Err(TomeError::NotFound(format!("module {id}")));
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<InstalledModule>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT spec_json, installed_at, updated_at,
                    (SELECT COUNT(*) FROM module_members WHERE module_id = ?1)
                 FROM modules WHERE id = ?1",
                params![id],
                |row| {
                    let spec_json: String = row.get(0)?;
                    let installed_at: i64 = row.get(1)?;
                    let updated_at: i64 = row.get(2)?;
                    let count: i64 = row.get(3)?;
                    Ok((spec_json, installed_at, updated_at, count))
                },
            )
            .optional()
            .map_err(|e| TomeError::Storage(format!("get module: {e}")))?;

        match row {
            None => Ok(None),
            Some((json, installed, updated, count)) => {
                let spec: ModuleSpec = serde_json::from_str(&json)
                    .map_err(|e| TomeError::Storage(format!("deserialize spec: {e}")))?;
                Ok(Some(InstalledModule {
                    spec,
                    member_count: count as u64,
                    installed_at: installed,
                    updated_at: updated,
                }))
            }
        }
    }

    pub fn list(&self) -> Result<Vec<InstalledModule>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.spec_json, m.installed_at, m.updated_at,
                    (SELECT COUNT(*) FROM module_members WHERE module_id = m.id)
                 FROM modules m ORDER BY m.name",
            )
            .map_err(|e| TomeError::Storage(format!("prepare list: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let _id: String = row.get(0)?;
                let spec_json: String = row.get(1)?;
                let installed: i64 = row.get(2)?;
                let updated: i64 = row.get(3)?;
                let count: i64 = row.get(4)?;
                Ok((spec_json, installed, updated, count))
            })
            .map_err(|e| TomeError::Storage(format!("query list: {e}")))?;

        let mut out = Vec::new();
        for row in rows {
            let (json, installed, updated, count) =
                row.map_err(|e| TomeError::Storage(format!("row: {e}")))?;
            let spec: ModuleSpec = serde_json::from_str(&json)
                .map_err(|e| TomeError::Storage(format!("deserialize spec: {e}")))?;
            out.push(InstalledModule {
                spec,
                member_count: count as u64,
                installed_at: installed,
                updated_at: updated,
            });
        }
        Ok(out)
    }

    pub fn members(&self, id: &str) -> Result<Vec<String>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT title FROM module_members WHERE module_id = ?1 ORDER BY title")
            .map_err(|e| TomeError::Storage(format!("prepare members: {e}")))?;
        let rows = stmt
            .query_map(params![id], |row| row.get::<_, String>(0))
            .map_err(|e| TomeError::Storage(format!("query members: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| TomeError::Storage(format!("row: {e}")))?);
        }
        Ok(out)
    }

    /// Find every module that lists `title` as a member. Useful when computing
    /// disk-usage-per-module or deciding whether an article is still needed.
    pub fn modules_for_title(&self, title: &str) -> Result<Vec<String>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT module_id FROM module_members WHERE title = ?1")
            .map_err(|e| TomeError::Storage(format!("prepare modules_for_title: {e}")))?;
        let rows = stmt
            .query_map(params![title], |row| row.get::<_, String>(0))
            .map_err(|e| TomeError::Storage(format!("query modules_for_title: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| TomeError::Storage(format!("row: {e}")))?);
        }
        Ok(out)
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use tome_core::Tier;

    use super::*;
    use crate::spec::CategorySpec;

    fn sample_spec() -> ModuleSpec {
        ModuleSpec {
            id: "physics-101".into(),
            name: "Physics 101".into(),
            description: Some("Intro physics".into()),
            default_tier: Tier::Warm,
            categories: vec![CategorySpec {
                name: "Physics".into(),
                depth: 1,
            }],
            explicit_titles: vec!["Newton's laws".into()],
        }
    }

    #[test]
    fn install_then_get() {
        let store = ModuleStore::open_in_memory().unwrap();
        let spec = sample_spec();
        store
            .install(&spec, &["Photon".into(), "Electron".into()])
            .unwrap();
        let got = store.get("physics-101").unwrap().unwrap();
        assert_eq!(got.spec, spec);
        assert_eq!(got.member_count, 2);
    }

    #[test]
    fn install_replaces_members() {
        let store = ModuleStore::open_in_memory().unwrap();
        let spec = sample_spec();
        store
            .install(&spec, &["Photon".into(), "Electron".into()])
            .unwrap();
        store.install(&spec, &["Quark".into()]).unwrap();
        let members = store.members("physics-101").unwrap();
        assert_eq!(members, vec!["Quark".to_string()]);
    }

    #[test]
    fn uninstall_removes_module_and_members() {
        let store = ModuleStore::open_in_memory().unwrap();
        let spec = sample_spec();
        store.install(&spec, &["Photon".into()]).unwrap();
        store.uninstall("physics-101").unwrap();
        assert!(store.get("physics-101").unwrap().is_none());
        assert!(store.members("physics-101").unwrap().is_empty());
    }

    #[test]
    fn uninstall_unknown_errors() {
        let store = ModuleStore::open_in_memory().unwrap();
        let err = store.uninstall("ghost").unwrap_err();
        assert!(matches!(err, TomeError::NotFound(_)));
    }

    #[test]
    fn list_orders_by_name() {
        let store = ModuleStore::open_in_memory().unwrap();
        for (id, name) in [("zeta", "Zeta"), ("alpha", "Alpha"), ("middle", "Middle")] {
            let spec = ModuleSpec {
                id: id.into(),
                name: name.into(),
                description: None,
                default_tier: Tier::Cold,
                categories: vec![],
                explicit_titles: vec!["X".into()],
            };
            store.install(&spec, &["X".into()]).unwrap();
        }
        let listed: Vec<_> = store
            .list()
            .unwrap()
            .iter()
            .map(|m| m.spec.name.clone())
            .collect();
        assert_eq!(listed, vec!["Alpha", "Middle", "Zeta"]);
    }

    #[test]
    fn modules_for_title_finds_all_owners() {
        let store = ModuleStore::open_in_memory().unwrap();
        let mut a = sample_spec();
        a.id = "a".into();
        let mut b = sample_spec();
        b.id = "b".into();
        b.name = "B module".into();
        store
            .install(&a, &["Photon".into(), "Electron".into()])
            .unwrap();
        store
            .install(&b, &["Photon".into(), "Quark".into()])
            .unwrap();
        let mut owners = store.modules_for_title("Photon").unwrap();
        owners.sort();
        assert_eq!(owners, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            store.modules_for_title("Quark").unwrap(),
            vec!["b".to_string()]
        );
    }

    #[test]
    fn install_validates_spec() {
        let store = ModuleStore::open_in_memory().unwrap();
        let mut spec = sample_spec();
        spec.id = "Bad ID".into(); // not kebab-case
        let err = store.install(&spec, &[]).unwrap_err();
        assert!(matches!(err, TomeError::Other(_)));
    }
}
