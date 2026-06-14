//! The Tome bookmark backup format — a versioned, futureproof JSON envelope
//! for exporting and importing bookmarks + folders.
//!
//! # Why this exists and what it guarantees
//!
//! A user's bookmarks are theirs. A backup taken today must remain readable by
//! every future version of Tome, no matter how the internal database schema
//! evolves. This module is the single, well-tested place where that promise
//! lives. The guarantees:
//!
//! 1. **Identity by name, never by id.** A bookmark references its folder by
//!    *name*; a folder references its parent by *name*. The article itself is
//!    keyed by its canonical *title*. SQLite rowids and Wikipedia page_ids are
//!    local to one database and rotate when a dump is re-ingested — names and
//!    titles are the durable, portable identities. This is what lets a backup
//!    survive a reinstall, a fresh dump, or a move to another machine.
//!
//! 2. **A monotonic [`CURRENT_FORMAT_VERSION`] plus a migration chain.** On
//!    import we parse to a generic JSON value, run it forward through
//!    `v1 → v2 → … → current` adapters (see [`migrate_to_current`]), and only
//!    then deserialize. New versions ADD adapters and never delete old ones, so
//!    any future build can always read any older backup — exactly the
//!    discipline the SQLite schema migration runner already uses.
//!
//! 3. **Forward tolerance.** Optional fields carry `#[serde(default)]` and we
//!    never use `deny_unknown_fields`, so a backup written by a *newer* Tome
//!    (with extra fields, or a higher version) is imported on a best-effort
//!    basis with a [`ParsedBackup::from_newer_version`] flag rather than
//!    hard-failing. A backup is never rendered "unusable."
//!
//! 4. **Lossless round-trip.** Everything a bookmark or folder carries today —
//!    title, note, folder membership, parent nesting, and original
//!    `created_at` timestamps — is captured and restored.
//!
//! The on-disk shape (version 1):
//!
//! ```json
//! {
//!   "app": "tome",
//!   "kind": "bookmarks",
//!   "format_version": 1,
//!   "exported_at": 1748500000,
//!   "folders":   [{ "name": "Survival", "parent": null, "created_at": 1748000000 }],
//!   "bookmarks": [{ "title": "Water purification", "folder": "Survival", "note": null, "created_at": 1748100000 }]
//! }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tome_core::{Result, TomeError};
use tome_storage::{Bookmark, BookmarkFolder, ImportBookmark, ImportFolder};

/// The current on-disk format version this build writes. Bump this ONLY when
/// adding a new shape, and in the same change add a `migrate_step` arm that
/// upgrades the previous version to this one. Never remove an old arm.
pub const CURRENT_FORMAT_VERSION: u32 = 1;

/// Discriminators stamped into every export so a stray JSON file picked by
/// mistake is rejected with a clear message instead of importing garbage.
pub const APP_TAG: &str = "tome";
pub const KIND_TAG: &str = "bookmarks";

/// The full backup envelope. Field order here is also the serialized order
/// (serde preserves struct field order), keeping exports human-readable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookmarkExport {
    /// Always [`APP_TAG`] in files we write. Tolerated-if-absent on read.
    #[serde(default)]
    pub app: String,
    /// Always [`KIND_TAG`] in files we write. Tolerated-if-absent on read.
    #[serde(default)]
    pub kind: String,
    /// The format version. This field is REQUIRED on read (no default): its
    /// absence is how we distinguish a real backup from an arbitrary JSON file.
    pub format_version: u32,
    /// Unix epoch seconds the backup was taken. Informational only.
    #[serde(default)]
    pub exported_at: Option<i64>,
    #[serde(default)]
    pub folders: Vec<ExportFolder>,
    #[serde(default)]
    pub bookmarks: Vec<ExportBookmark>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportFolder {
    pub name: String,
    /// Parent folder's NAME, or null for a root-level folder.
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportBookmark {
    /// Canonical article title — the durable identity.
    pub title: String,
    /// Owning folder's NAME, or null for unfiled.
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub created_at: i64,
}

/// Summary returned to the UI after a successful export.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExportSummary {
    pub path: String,
    pub folders: u64,
    pub bookmarks: u64,
    pub format_version: u32,
}

/// Summary returned to the UI after a successful import. Combines the storage
/// layer's [`tome_storage::ImportOutcome`] counts with the version metadata so
/// the UI can warn when a backup came from a newer Tome.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ImportSummary {
    pub folders_created: u64,
    pub folders_matched: u64,
    pub bookmarks_added: u64,
    pub bookmarks_skipped: u64,
    /// The `format_version` the file declared.
    pub source_format_version: u32,
    /// True when the file's version is newer than this build understands and
    /// was imported best-effort. The UI surfaces this as a gentle warning.
    pub from_newer_version: bool,
}

/// The result of parsing a backup file: the (possibly migrated) envelope plus
/// the version metadata the caller needs to build an [`ImportSummary`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedBackup {
    pub export: BookmarkExport,
    pub source_format_version: u32,
    pub from_newer_version: bool,
}

impl BookmarkExport {
    /// Build an envelope from live storage rows. `id_to_name` resolution is
    /// done here so the rest of the format logic never sees a numeric id.
    pub fn build(
        folders: &[BookmarkFolder],
        bookmarks: &[Bookmark],
        exported_at: Option<i64>,
    ) -> Self {
        // id -> name lookup so we can express parent/folder links by name.
        let id_to_name: std::collections::HashMap<i64, String> =
            folders.iter().map(|f| (f.id, f.name.clone())).collect();

        let folders = folders
            .iter()
            .map(|f| ExportFolder {
                name: f.name.clone(),
                parent: f.parent_id.and_then(|p| id_to_name.get(&p).cloned()),
                created_at: f.created_at,
            })
            .collect();

        let bookmarks = bookmarks
            .iter()
            .map(|b| ExportBookmark {
                title: b.article_title.clone(),
                folder: b.folder_id.and_then(|f| id_to_name.get(&f).cloned()),
                note: b.note.clone(),
                created_at: b.created_at,
            })
            .collect();

        Self {
            app: APP_TAG.to_string(),
            kind: KIND_TAG.to_string(),
            format_version: CURRENT_FORMAT_VERSION,
            exported_at,
            folders,
            bookmarks,
        }
    }

    /// Serialize to pretty (human-readable) JSON.
    pub fn to_pretty_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| TomeError::Other(format!("serialize bookmark backup: {e}")))
    }

    /// Convert the envelope into the storage layer's import DTOs.
    pub fn into_storage(self) -> (Vec<ImportFolder>, Vec<ImportBookmark>) {
        let folders = self
            .folders
            .into_iter()
            .map(|f| ImportFolder {
                name: f.name,
                parent_name: f.parent,
                created_at: f.created_at,
            })
            .collect();
        let bookmarks = self
            .bookmarks
            .into_iter()
            .map(|b| ImportBookmark {
                title: b.title,
                folder_name: b.folder,
                note: b.note,
                created_at: b.created_at,
            })
            .collect();
        (folders, bookmarks)
    }
}

/// Parse a backup file's text into a validated, version-migrated envelope.
///
/// Errors (with clear messages) when the text isn't JSON, isn't a Tome
/// bookmark backup, or is structurally unreadable. A backup from a *newer*
/// Tome does NOT error — it parses best-effort with `from_newer_version`.
pub fn parse(text: &str) -> Result<ParsedBackup> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| TomeError::Other(format!("backup is not valid JSON: {e}")))?;

    // A real backup always carries an integer `format_version`. Its absence is
    // the cheapest reliable signal that the user picked the wrong file.
    let source_format_version = value
        .get("format_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            TomeError::Other(
                "this file is not a Tome bookmark backup (no \"format_version\" field)".into(),
            )
        })? as u32;

    // If the file declares an app/kind, it must be ours. Absent → tolerated
    // (could be a hand-authored or very early file).
    if let Some(app) = value.get("app").and_then(Value::as_str) {
        if !app.is_empty() && app != APP_TAG {
            return Err(TomeError::Other(format!(
                "this backup belongs to another app (app = {app:?}), not Tome"
            )));
        }
    }
    if let Some(kind) = value.get("kind").and_then(Value::as_str) {
        if !kind.is_empty() && kind != KIND_TAG {
            return Err(TomeError::Other(format!(
                "this Tome backup is a {kind:?} backup, not bookmarks"
            )));
        }
    }

    let from_newer_version = source_format_version > CURRENT_FORMAT_VERSION;

    // Run known upgrade adapters forward to the current shape. For a
    // newer-than-known file there are no adapters to run; serde's tolerance of
    // unknown fields handles the best-effort read below.
    let migrated = migrate_to_current(value, source_format_version);

    let export: BookmarkExport = serde_json::from_value(migrated)
        .map_err(|e| TomeError::Other(format!("backup structure could not be read: {e}")))?;

    Ok(ParsedBackup {
        export,
        source_format_version,
        from_newer_version,
    })
}

/// Drive a parsed value forward from `from` to [`CURRENT_FORMAT_VERSION`] by
/// applying each single-step adapter in turn. A no-op when `from` is already
/// current or newer.
fn migrate_to_current(mut value: Value, from: u32) -> Value {
    let mut v = from;
    while v < CURRENT_FORMAT_VERSION {
        value = migrate_step(v, value);
        v += 1;
    }
    value
}

/// Upgrade a value from `from_version` to `from_version + 1`.
///
/// Today there is only version 1, so this is a no-op. When the format grows,
/// add an arm here (e.g. `1 => migrate_v1_to_v2(value)`) and NEVER remove an
/// existing one — that append-only chain is what keeps every old backup
/// readable forever.
// The single `_` arm is intentional: this is an append-only extension point.
// Each future format version adds one arm here; the match shape must stay so
// the diff for a new version is a single added line.
#[allow(clippy::match_single_binding)]
fn migrate_step(from_version: u32, value: Value) -> Value {
    match from_version {
        // 1 => migrate_v1_to_v2(value),
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: i64, name: &str, parent_id: Option<i64>) -> BookmarkFolder {
        BookmarkFolder {
            id,
            name: name.into(),
            parent_id,
            created_at: 100,
        }
    }
    fn bm(id: i64, title: &str, folder_id: Option<i64>, note: Option<&str>) -> Bookmark {
        Bookmark {
            id,
            article_title: title.into(),
            folder_id,
            note: note.map(Into::into),
            created_at: 200,
        }
    }

    #[test]
    fn build_then_parse_round_trips_losslessly() {
        let folders = vec![folder(1, "Survival", None), folder(2, "Water", Some(1))];
        let bookmarks = vec![
            bm(1, "Water purification", Some(2), Some("boil 1 min")),
            bm(2, "Photon", None, None),
        ];
        let export = BookmarkExport::build(&folders, &bookmarks, Some(1_700_000_000));
        let json = export.to_pretty_json().unwrap();

        let parsed = parse(&json).unwrap();
        assert_eq!(parsed.source_format_version, CURRENT_FORMAT_VERSION);
        assert!(!parsed.from_newer_version);
        // Folder links expressed by name.
        let water = parsed
            .export
            .folders
            .iter()
            .find(|f| f.name == "Water")
            .unwrap();
        assert_eq!(water.parent.as_deref(), Some("Survival"));
        // Bookmark links expressed by folder name; note + title preserved.
        let wp = parsed
            .export
            .bookmarks
            .iter()
            .find(|b| b.title == "Water purification")
            .unwrap();
        assert_eq!(wp.folder.as_deref(), Some("Water"));
        assert_eq!(wp.note.as_deref(), Some("boil 1 min"));
        assert_eq!(parsed.export.exported_at, Some(1_700_000_000));
    }

    #[test]
    fn export_carries_app_kind_and_version_tags() {
        let export = BookmarkExport::build(&[], &[], None);
        let json = export.to_pretty_json().unwrap();
        assert!(json.contains("\"app\": \"tome\""));
        assert!(json.contains("\"kind\": \"bookmarks\""));
        assert!(json.contains("\"format_version\": 1"));
    }

    #[test]
    fn into_storage_maps_names_through() {
        let folders = vec![folder(1, "A", None)];
        let bookmarks = vec![bm(1, "T", Some(1), None)];
        let export = BookmarkExport::build(&folders, &bookmarks, None);
        let (ifolders, ibms) = export.into_storage();
        assert_eq!(ifolders[0].name, "A");
        assert_eq!(ibms[0].title, "T");
        assert_eq!(ibms[0].folder_name.as_deref(), Some("A"));
    }

    #[test]
    fn parse_rejects_non_json() {
        let err = parse("definitely not json").unwrap_err();
        assert!(format!("{err}").contains("not valid JSON"));
    }

    #[test]
    fn parse_rejects_json_without_format_version() {
        // Valid JSON, but not one of our backups.
        let err = parse(r#"{"hello": "world"}"#).unwrap_err();
        assert!(format!("{err}").contains("not a Tome bookmark backup"));
    }

    #[test]
    fn parse_rejects_wrong_app() {
        let err = parse(r#"{"format_version": 1, "app": "someotherapp"}"#).unwrap_err();
        assert!(format!("{err}").contains("another app"));
    }

    #[test]
    fn parse_rejects_wrong_kind() {
        let err = parse(r#"{"format_version": 1, "app": "tome", "kind": "history"}"#).unwrap_err();
        assert!(format!("{err}").contains("not bookmarks"));
    }

    #[test]
    fn parse_tolerates_missing_app_and_kind() {
        // A minimal/hand-authored file with just the version and a bookmark.
        let parsed = parse(r#"{"format_version": 1, "bookmarks": [{"title": "Photon"}]}"#).unwrap();
        assert_eq!(parsed.export.bookmarks.len(), 1);
        assert_eq!(parsed.export.bookmarks[0].title, "Photon");
        // Missing folder/note default cleanly.
        assert_eq!(parsed.export.bookmarks[0].folder, None);
    }

    #[test]
    fn parse_tolerates_unknown_fields_for_forward_compat() {
        // A future field we don't know about must be ignored, not fatal.
        let json = r#"{
            "app": "tome", "kind": "bookmarks", "format_version": 1,
            "future_field": {"nested": true},
            "bookmarks": [{"title": "Photon", "tags": ["physics"]}]
        }"#;
        let parsed = parse(json).unwrap();
        assert_eq!(parsed.export.bookmarks[0].title, "Photon");
        assert!(!parsed.from_newer_version);
    }

    #[test]
    fn parse_flags_a_newer_version_but_still_imports() {
        // A backup from a hypothetical future Tome (version 99). We can't run
        // adapters we don't have, but we must still recover what we recognize.
        let json = r#"{"app": "tome", "kind": "bookmarks", "format_version": 99,
                       "bookmarks": [{"title": "Photon"}]}"#;
        let parsed = parse(json).unwrap();
        assert_eq!(parsed.source_format_version, 99);
        assert!(parsed.from_newer_version);
        assert_eq!(parsed.export.bookmarks.len(), 1);
    }
}
