//! User-supplied multistream dump and index files MUST NEVER be modified.
//!
//! Same invariant as `tome-services/tests/file_integrity.rs`, just for the
//! dump-access layer. The Wikipedia dump in particular is the irreplaceable
//! file in the offline-only scenario — if Tome corrupts it, a user without
//! a backup is stuck.

use std::fs;
use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;
use tome_dump::fixture::{PageData, build_fixture};
use tome_dump::{DumpReader, IndexReader};

fn assert_unchanged_after<F, T>(path: &Path, op: F) -> T
where
    F: FnOnce() -> T,
{
    let before = fs::read(path).expect("read before-bytes");

    let mut perms = fs::metadata(path).expect("stat").permissions();
    perms.set_readonly(true);
    fs::set_permissions(path, perms).expect("set readonly");

    let result = op();

    let mut perms = fs::metadata(path).expect("stat after").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(path, perms).expect("restore writable");

    let after = fs::read(path).expect("read after-bytes");
    assert_eq!(before.len(), after.len(), "file {path:?} length changed");
    assert_eq!(before, after, "file {path:?} contents changed");

    result
}

fn fixture() -> tome_dump::fixture::Fixture {
    build_fixture(
        vec![
            PageData {
                title: "Photon".into(),
                page_id: 23535,
                revision_id: 987654321,
                wikitext: "A '''photon''' is an [[elementary particle]].".into(),
            },
            PageData {
                title: "Electron".into(),
                page_id: 9404,
                revision_id: 111222333,
                wikitext: "An '''electron''' is a subatomic particle.".into(),
            },
        ],
        2,
    )
    .unwrap()
}

#[test]
fn read_stream_does_not_mutate_dump_file() {
    let fx = fixture();
    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();

    let reader = DumpReader::open(dump_file.path());

    assert_unchanged_after(dump_file.path(), || {
        let bytes = reader
            .read_stream(fx.streams[0].stream_offset, None)
            .expect("read stream");
        assert!(!bytes.is_empty());
    });
}

#[test]
fn read_stream_with_bounded_length_does_not_mutate_dump_file() {
    let fx = fixture();
    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();

    let reader = DumpReader::open(dump_file.path());

    assert_unchanged_after(dump_file.path(), || {
        let _ = reader.read_stream(fx.streams[0].stream_offset, Some(1024 * 1024));
    });
}

#[test]
fn many_repeated_reads_do_not_mutate_dump_file() {
    let fx = fixture();
    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();

    let reader = DumpReader::open(dump_file.path());

    assert_unchanged_after(dump_file.path(), || {
        for _ in 0..50 {
            let _ = reader
                .read_stream(fx.streams[0].stream_offset, None)
                .unwrap();
        }
    });
}

#[test]
fn index_reader_does_not_mutate_index_file() {
    let fx = fixture();
    let mut index_file = NamedTempFile::new().unwrap();
    index_file.write_all(&fx.index_bytes).unwrap();
    index_file.flush().unwrap();

    assert_unchanged_after(index_file.path(), || {
        let file = std::fs::File::open(index_file.path()).expect("open");
        let entries: Vec<_> = IndexReader::new(file)
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        assert!(!entries.is_empty());
    });
}

/// Same self-check as tome-services: prove the harness catches mutations.
#[test]
#[should_panic(expected = "changed")]
fn harness_detects_a_mutation_when_one_happens() {
    let f = NamedTempFile::new().unwrap();
    fs::write(f.path(), b"original_bytes_here").unwrap();

    assert_unchanged_after(f.path(), || {
        let mut perms = fs::metadata(f.path()).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(f.path(), perms).unwrap();
        fs::write(f.path(), b"CORRUPTED_BYTES_NOPE").unwrap();
    });
}

#[test]
fn read_stream_at_invalid_offset_returns_error_without_mutation() {
    let fx = fixture();
    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();

    let reader = DumpReader::open(dump_file.path());

    assert_unchanged_after(dump_file.path(), || {
        let len = fx.dump_bytes.len() as u64;
        let result = reader.read_stream(len + 1024, None);
        assert!(result.is_err(), "out-of-range offset must error");
    });
}
