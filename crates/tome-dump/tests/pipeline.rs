//! End-to-end integration test for the dump access layer.
//!
//! Builds a synthetic multistream fixture in memory, writes it to temp files,
//! and exercises the index reader, dump reader, and page parser against it
//! without any real Wikipedia data.

use std::io::Write;

use tempfile::NamedTempFile;
use tome_dump::fixture::{PageData, build_fixture};
use tome_dump::{DumpReader, IndexReader, parse_pages, verify_sha1};

fn pages_for_test() -> Vec<PageData> {
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
        PageData {
            title: "Quark".into(),
            page_id: 25313,
            revision_id: 444555666,
            wikitext: "A '''quark''' is an elementary particle.".into(),
        },
        PageData {
            title: "Higgs boson".into(),
            page_id: 35179,
            revision_id: 777888999,
            wikitext: "The '''Higgs boson''' is an elementary particle in the Standard Model."
                .into(),
        },
    ]
}

#[test]
fn round_trip_index_yields_all_entries() {
    let fx = build_fixture(pages_for_test(), 2).unwrap();
    let mut index_file = NamedTempFile::new().unwrap();
    index_file.write_all(&fx.index_bytes).unwrap();
    index_file.flush().unwrap();

    let file = std::fs::File::open(index_file.path()).unwrap();
    let entries: Vec<_> = IndexReader::new(file)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].title, "Photon");
    assert_eq!(entries[1].title, "Electron");
    assert_eq!(entries[2].title, "Quark");
    assert_eq!(entries[3].title, "Higgs boson");

    // Pages 1-2 share stream 0, pages 3-4 share stream 1.
    assert_eq!(entries[0].stream_offset, entries[1].stream_offset);
    assert_eq!(entries[2].stream_offset, entries[3].stream_offset);
    assert_ne!(entries[0].stream_offset, entries[2].stream_offset);
}

#[test]
fn extract_individual_stream_and_parse_pages() {
    let pages = pages_for_test();
    let fx = build_fixture(pages.clone(), 2).unwrap();

    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();

    let reader = DumpReader::open(dump_file.path());

    // Read stream 0 (Photon + Electron) by offset only.
    let stream0_bytes = reader
        .read_stream(fx.streams[0].stream_offset, None)
        .unwrap();
    let stream0_pages = parse_pages(&stream0_bytes).unwrap();
    assert_eq!(stream0_pages.len(), 2);
    assert_eq!(stream0_pages[0].title, "Photon");
    assert_eq!(stream0_pages[0].page_id, 23535);
    assert_eq!(stream0_pages[0].revision_id, 987654321);
    assert!(stream0_pages[0].wikitext.contains("'''photon'''"));
    assert_eq!(stream0_pages[1].title, "Electron");

    // Read stream 1 (Quark + Higgs boson) with bounded length.
    let stream1_bytes = reader
        .read_stream(
            fx.streams[1].stream_offset,
            Some(fx.streams[1].stream_length),
        )
        .unwrap();
    let stream1_pages = parse_pages(&stream1_bytes).unwrap();
    assert_eq!(stream1_pages.len(), 2);
    assert_eq!(stream1_pages[0].title, "Quark");
    assert_eq!(stream1_pages[1].title, "Higgs boson");
}

#[test]
fn streams_are_independently_decodable_in_any_order() {
    let fx = build_fixture(pages_for_test(), 1).unwrap();
    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();

    let reader = DumpReader::open(dump_file.path());

    // Read in reverse order to prove streams are independent.
    for info in fx.streams.iter().rev() {
        let bytes = reader
            .read_stream(info.stream_offset, Some(info.stream_length))
            .unwrap();
        let parsed = parse_pages(&bytes).unwrap();
        assert_eq!(parsed.len(), info.pages.len());
        for (got, want) in parsed.iter().zip(info.pages.iter()) {
            assert_eq!(got.title, want.title);
            assert_eq!(got.page_id, want.page_id);
            assert_eq!(got.revision_id, want.revision_id);
        }
    }
}

#[test]
fn sha1_verification_against_synthetic_dump() {
    use sha1::{Digest, Sha1};

    let fx = build_fixture(pages_for_test(), 2).unwrap();
    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();

    let mut hasher = Sha1::new();
    hasher.update(&fx.dump_bytes);
    let expected = hex::encode(hasher.finalize());

    verify_sha1(dump_file.path(), &expected).unwrap();

    let bad = "0000000000000000000000000000000000000000";
    let err = verify_sha1(dump_file.path(), bad).unwrap_err();
    assert!(matches!(err, tome_core::TomeError::Integrity(_)));
}
