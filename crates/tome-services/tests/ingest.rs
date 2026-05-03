//! Integration test for dump ingestion.
//!
//! Uses the in-memory fixture from `tome-dump` to produce a synthetic
//! multistream index, writes it to a temp file, then drives the ingestion
//! pipeline through the `Tome` facade and confirms storage reflects the
//! Cold-tier entries.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tempfile::{NamedTempFile, TempDir};
use tome_api::testing::MockTransport;
use tome_api::{ClientConfig, KillSwitch, MediaWikiClient};
use tome_config::Settings;
use tome_core::Title;
use tome_dump::fixture::{PageData, build_fixture};
use tome_modules::ModuleStore;
use tome_search::Index as SearchIndex;
use tome_services::Tome;
use tome_storage::{ArchiveStore, ArticleStore, SqliteArticleStore};

fn pages() -> Vec<PageData> {
    (1..=50)
        .map(|i| PageData {
            title: format!("Article{i}"),
            page_id: i,
            revision_id: i * 1000,
            wikitext: format!("Body of article {i}."),
        })
        .collect()
}

fn build_facade() -> (Tome, NamedTempFile, NamedTempFile, TempDir) {
    let fx = build_fixture(pages(), 10).unwrap();
    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();
    let mut index_file = NamedTempFile::new().unwrap();
    index_file.write_all(&fx.index_bytes).unwrap();
    index_file.flush().unwrap();

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
    Settings {
        dump_path: Some(dump_file.path().to_path_buf()),
        last_index_path: None,
        recommendations_enabled: true,
        map_source_path: None,
    }
    .save(data_dir.path())
    .unwrap();

    // Skip the API attempt for Cold reads so the test doesn't burn the
    // full backoff schedule against the empty MockTransport.
    let tome = Tome::new(
        storage,
        archive,
        modules,
        search,
        api,
        data_dir.path().to_path_buf(),
    )
    .with_prefer_api_for_cold(false);
    (tome, dump_file, index_file, data_dir)
}

#[test]
fn ingest_populates_cold_tier_for_every_index_entry() {
    let (tome, _dump, index, _data_dir) = build_facade();
    let progress_calls = AtomicU64::new(0);
    let summary = tome
        .ingest_index(index.path(), |_count| {
            progress_calls.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

    assert_eq!(summary.entries_processed, 50);
    let counts = tome.tier_counts().unwrap();
    assert_eq!(counts.cold, 50);
    assert_eq!(counts.hot, 0);
    assert_eq!(counts.warm, 0);

    // The on_progress callback fires at least once (the final flush).
    assert!(progress_calls.load(Ordering::SeqCst) >= 1);
}

#[test]
fn ingest_then_lookup_reads_back_a_seeded_article() {
    let (tome, _dump, index, _data_dir) = build_facade();
    tome.ingest_index(index.path(), |_| {}).unwrap();

    // Use the search facility to confirm the article landed.
    // Search needs a built Tantivy index — but the ingest path here only
    // populates storage. So instead we use the read_article async path
    // through a manual runtime, since storage holds the metadata.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build runtime");
    let resp = rt
        .block_on(tome.read_article(&Title::new("Article25")))
        .expect("article should resolve through dump fallback");
    assert_eq!(resp.title, "Article25");
}

#[test]
fn re_ingest_updates_existing_offsets_in_place() {
    let (tome, _dump, index, _data_dir) = build_facade();
    tome.ingest_index(index.path(), |_| {}).unwrap();
    let first = tome.tier_counts().unwrap().cold;

    // Re-running should not duplicate rows (UNIQUE on page_id + ON CONFLICT).
    tome.ingest_index(index.path(), |_| {}).unwrap();
    let second = tome.tier_counts().unwrap().cold;
    assert_eq!(first, second);
}
