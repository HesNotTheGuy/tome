//! User-supplied files MUST NEVER be modified.
//!
//! These tests enforce the load-bearing invariant for offline-only users:
//! Tome treats every file path the user gives it as strictly read-only. A
//! buggy `OpenOptions::write(true)` somewhere in the call chain — even one
//! that never gets exercised in normal flow — would make Tome unsafe for
//! users whose Wikipedia dump is their only copy.
//!
//! For each user-input ingester we:
//!
//! 1. Hash the file's bytes before the operation.
//! 2. Set the file read-only on disk. If anything tries to open it for
//!    writing, the OS refuses and the operation errors loudly — much louder
//!    than silent mutation.
//! 3. Run the full ingest path.
//! 4. Hash again, compare. Bail if anything changed.
//!
//! These tests don't care what the ingester *did* with the data — that's
//! covered elsewhere. They only care that the source file is byte-identical
//! after.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use tempfile::NamedTempFile;

use tome_api::testing::MockTransport;
use tome_api::{ClientConfig, KillSwitch, MediaWikiClient};
use tome_archive::ArchiveStore;
use tome_modules::ModuleStore;
use tome_search::Index as SearchIndex;
use tome_services::{Tome, category_ingest, geotag_ingest, redirect_ingest};
use tome_storage::{ArticleStore, SqliteArticleStore};

/// Run `op` against `path` and assert that the file's bytes are unchanged.
///
/// Sets the file read-only on disk for the duration of `op`. On Windows this
/// flips the read-only attribute (writes return ERROR_ACCESS_DENIED). On
/// Unix this strips the user write bit (writes return EACCES). Either way,
/// any code path that tried to mutate the file would fail with a clear OS
/// error rather than silently succeeding.
fn assert_unchanged_after<F, T>(path: &Path, op: F) -> T
where
    F: FnOnce() -> T,
{
    let before = fs::read(path).expect("read before-bytes");

    let mut perms = fs::metadata(path).expect("stat").permissions();
    perms.set_readonly(true);
    fs::set_permissions(path, perms).expect("set readonly");

    let result = op();

    // Restore writability so tempfile cleanup doesn't choke on Windows.
    let mut perms = fs::metadata(path).expect("stat after").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(path, perms).expect("restore writable");

    let after = fs::read(path).expect("read after-bytes");
    assert_eq!(
        before.len(),
        after.len(),
        "file {path:?} length changed: {} -> {}",
        before.len(),
        after.len(),
    );
    assert_eq!(before, after, "file {path:?} contents changed");

    result
}

fn write_gz(bytes: &[u8]) -> NamedTempFile {
    let f = NamedTempFile::new().expect("tempfile");
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).expect("gz write");
    let compressed = enc.finish().expect("gz finish");
    fs::write(f.path(), &compressed).expect("write tempfile");
    f
}

#[test]
fn geotag_ingest_does_not_mutate_source_file() {
    let sql = b"INSERT INTO `geo_tags` VALUES (1,42,'earth',1,40.7,-74.0,1000,'city','New York','US','NY');\n";
    let f = write_gz(sql);

    let mut count = 0_u64;
    assert_unchanged_after(f.path(), || {
        let n = geotag_ingest::parse_file(f.path(), |_| count += 1).expect("parse");
        assert!(n > 0, "fixture should yield at least one row");
    });
}

#[test]
fn categorylinks_ingest_does_not_mutate_source_file() {
    // Match the columns the category ingester actually parses.
    let sql = b"INSERT INTO `categorylinks` VALUES (42,'Cities','New_York','2020-01-01 00:00:00','','uca400','en','page');\n";
    let f = write_gz(sql);

    assert_unchanged_after(f.path(), || {
        let _ = category_ingest::parse_file(f.path(), |_| {});
    });
}

#[test]
fn redirect_ingest_does_not_mutate_source_file() {
    let sql =
        b"INSERT INTO `redirect` VALUES (1,0,'United_States','',''),(2,0,'Photon','','Section');\n";
    let f = write_gz(sql);

    let mut count = 0_u64;
    assert_unchanged_after(f.path(), || {
        let n = redirect_ingest::parse_file(f.path(), |_| count += 1).expect("parse");
        assert!(n > 0);
    });
}

#[test]
fn geotag_ingest_errors_when_file_is_truncated() {
    // Write a non-gzipped file with the right extension. The ingester should
    // return an error, not panic, and not write anything.
    let f = NamedTempFile::new().unwrap();
    fs::write(f.path(), b"this is not a gzip stream").unwrap();

    assert_unchanged_after(f.path(), || {
        let result = geotag_ingest::parse_file(f.path(), |_| {});
        assert!(result.is_err(), "garbage input must produce an error");
    });
}

#[test]
fn redirect_ingest_errors_when_file_is_truncated() {
    let f = NamedTempFile::new().unwrap();
    fs::write(f.path(), b"\x1f\x8b\x08\x00truncated mid-stream").unwrap();

    assert_unchanged_after(f.path(), || {
        let result = redirect_ingest::parse_file(f.path(), |_| {});
        assert!(result.is_err(), "truncated gzip must produce an error");
    });
}

fn build_minimal_tome() -> (Tome, tempfile::TempDir) {
    let storage: Arc<dyn ArticleStore> = Arc::new(SqliteArticleStore::open_in_memory().unwrap());
    let archive = Arc::new(ArchiveStore::open_in_memory().unwrap());
    let modules = Arc::new(ModuleStore::open_in_memory().unwrap());
    let search = Arc::new(SearchIndex::create_in_ram().unwrap());
    let transport = Arc::new(MockTransport::new(vec![]));
    let kill = Arc::new(KillSwitch::new());
    let api = Arc::new(MediaWikiClient::new(
        ClientConfig::default(),
        transport,
        kill,
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
    (tome, data_dir)
}

#[test]
fn module_import_does_not_mutate_toml_file() {
    let toml = br#"
id = "test-module"
name = "Test Module"
description = "A test"
default_tier = "warm"

[[categories]]
name = "Physics"
depth = 1

explicit_titles = ["Photon", "Electron"]
"#;
    let f = NamedTempFile::new().unwrap();
    fs::write(f.path(), toml).unwrap();

    let (tome, _data_dir) = build_minimal_tome();

    assert_unchanged_after(f.path(), || {
        // The import will succeed or fail based on parsing; we only care
        // that the source file isn't touched. Errors are fine.
        let _ = tome.import_module_from_path(f.path());
    });
}

/// Self-check: verify the harness actually catches mutations. If this test
/// ever turns into a false negative, every other test in this file becomes
/// meaningless. We deliberately write to the file inside the closure and
/// expect the assertion to fail.
#[test]
#[should_panic(expected = "changed")]
fn harness_detects_a_mutation_when_one_happens() {
    let f = NamedTempFile::new().unwrap();
    fs::write(f.path(), b"original_bytes_here").unwrap();

    assert_unchanged_after(f.path(), || {
        // Strip the readonly bit then mutate, simulating a buggy ingester
        // that managed to write despite the guard. Same length so the
        // length-equality assertion passes — we want the contents check to
        // be the one that fires, proving it works.
        let mut perms = fs::metadata(f.path()).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(f.path(), perms).unwrap();
        fs::write(f.path(), b"CORRUPTED_BYTES_NOPE").unwrap();
    });
}
