//! Bookmark backup is a durability promise: a backup taken by one install
//! must restore cleanly into a *fresh* install, forever, and a corrupt or
//! wrong file must never damage the bookmarks already present.
//!
//! Unit tests in `bookmark_export` and `store` cover the format and the
//! transaction. These tests cover the part only an end-to-end run can: a real
//! JSON file written to disk by one `Tome`, read back by a different one.

use std::sync::Arc;

use tome_api::testing::MockTransport;
use tome_api::{ClientConfig, KillSwitch, MediaWikiClient};
use tome_modules::ModuleStore;
use tome_search::Index as SearchIndex;
use tome_services::Tome;
use tome_storage::{ArchiveStore, ArticleStore, SqliteArticleStore};

/// Build a self-contained `Tome` over fresh in-memory stores. Each call is an
/// independent "install" with its own data dir, so a backup written by one and
/// imported by another exercises the real cross-install path.
fn fresh_tome() -> (Arc<Tome>, tempfile::TempDir) {
    let storage: Arc<dyn ArticleStore> = Arc::new(SqliteArticleStore::open_in_memory().unwrap());
    let archive = Arc::new(ArchiveStore::open_in_memory().unwrap());
    let modules = Arc::new(ModuleStore::open_in_memory().unwrap());
    let search = Arc::new(SearchIndex::create_in_ram().unwrap());
    let api = Arc::new(MediaWikiClient::new(
        ClientConfig::default(),
        Arc::new(MockTransport::new(vec![])),
        Arc::new(KillSwitch::new()),
    ));
    let data_dir = tempfile::tempdir().unwrap();
    let tome = Tome::new(
        storage,
        archive,
        modules,
        search,
        api,
        data_dir.path().to_path_buf(),
    );
    (Arc::new(tome), data_dir)
}

#[test]
fn backup_round_trips_into_a_fresh_install() {
    let (source, _src_dir) = fresh_tome();

    // Build a representative bookmark set on the "source" install.
    let survival = source.create_folder("Survival", None).unwrap();
    source
        .add_bookmark("Water purification", Some(survival), Some("boil 1 min"))
        .unwrap();
    source.add_bookmark("Photon", None, None).unwrap(); // unfiled

    let out_dir = tempfile::tempdir().unwrap();
    let backup_path = out_dir.path().join("backup.json");
    let summary = source.export_bookmarks(&backup_path).unwrap();
    assert_eq!(summary.folders, 1);
    assert_eq!(summary.bookmarks, 2);
    assert!(backup_path.exists(), "backup file was written");

    // A brand-new install with empty storage imports the backup.
    let (dest, _dst_dir) = fresh_tome();
    let imported = dest.import_bookmarks(&backup_path, false).unwrap();
    assert_eq!(imported.folders_created, 1);
    assert_eq!(imported.bookmarks_added, 2);
    assert!(!imported.from_newer_version);

    // The restored shape matches the source, resolved by NAME not by id.
    assert_eq!(dest.count_bookmarks().unwrap(), 2);
    let folders = dest.list_folders().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "Survival");
    let in_folder = dest.bookmarks_in_folder(Some(folders[0].id), 100).unwrap();
    assert_eq!(in_folder.len(), 1);
    assert_eq!(in_folder[0].article_title, "Water purification");
    assert_eq!(in_folder[0].note.as_deref(), Some("boil 1 min"));
    assert_eq!(dest.bookmarks_in_folder(None, 100).unwrap().len(), 1);
}

#[test]
fn re_importing_the_same_backup_is_a_safe_no_op() {
    let (source, _s) = fresh_tome();
    source.add_bookmark("Photon", None, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.json");
    source.export_bookmarks(&path).unwrap();

    let (dest, _d) = fresh_tome();
    dest.import_bookmarks(&path, false).unwrap();
    let second = dest.import_bookmarks(&path, false).unwrap();
    assert_eq!(second.bookmarks_added, 0);
    assert_eq!(second.bookmarks_skipped, 1);
    assert_eq!(dest.count_bookmarks().unwrap(), 1);
}

#[test]
fn replace_mode_swaps_in_the_backup_contents() {
    let (source, _s) = fresh_tome();
    source.add_bookmark("FromBackup", None, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.json");
    source.export_bookmarks(&path).unwrap();

    let (dest, _d) = fresh_tome();
    dest.add_bookmark("Pre-existing", None, None).unwrap();
    dest.import_bookmarks(&path, true).unwrap(); // replace
    assert_eq!(dest.count_bookmarks().unwrap(), 1);
    assert!(dest.is_bookmarked("FromBackup").unwrap());
    assert!(!dest.is_bookmarked("Pre-existing").unwrap());
}

#[test]
fn a_corrupt_backup_is_rejected_and_leaves_existing_bookmarks_intact() {
    let (dest, _d) = fresh_tome();
    dest.add_bookmark("Keep me", None, None).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garbage.json");
    std::fs::write(&path, b"this is not a backup file").unwrap();

    let err = dest.import_bookmarks(&path, false).unwrap_err();
    assert!(format!("{err}").contains("not valid JSON"));
    // The pre-existing bookmark must be untouched by the failed import.
    assert_eq!(dest.count_bookmarks().unwrap(), 1);
    assert!(dest.is_bookmarked("Keep me").unwrap());
}

#[test]
fn exporting_to_a_directory_writes_a_default_filename() {
    let (source, _s) = fresh_tome();
    source.add_bookmark("Photon", None, None).unwrap();
    let dir = tempfile::tempdir().unwrap();

    // Pass the directory itself — export should write tome-bookmarks.json into it.
    let summary = source.export_bookmarks(dir.path()).unwrap();
    assert!(summary.path.ends_with("tome-bookmarks.json"));
    assert!(dir.path().join("tome-bookmarks.json").exists());
}
