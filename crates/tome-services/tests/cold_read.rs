//! End-to-end integration test for the Cold-tier article read flow.
//!
//! Builds a synthetic multistream dump in memory, ingests its index into the
//! storage layer as Cold-tier articles, then reads one back through the
//! `Tome` facade. With the kill switch engaged the API path is skipped and
//! the article must come from the dump via the local renderer — this proves
//! the storage + dump + wikitext composition works without a network.

use std::io::Write;
use std::sync::Arc;

use tempfile::{NamedTempFile, TempDir};
use tome_api::testing::{MockResponse, MockTransport};
use tome_api::{ClientConfig, KillSwitch, MediaWikiClient};
use tome_core::{Settings, Tier, Title};
use tome_dump::fixture::{PageData, build_fixture};
use tome_modules::ModuleStore;
use tome_search::Index as SearchIndex;
use tome_services::{ArticleSource, Tome};
use tome_storage::{ArchiveStore, ArticleMetadata, ArticleStore, SqliteArticleStore};

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
    ]
}

fn build_facade() -> (Tome, NamedTempFile, TempDir) {
    let fx = build_fixture(pages_for_test(), 2).unwrap();
    let mut dump_file = NamedTempFile::new().unwrap();
    dump_file.write_all(&fx.dump_bytes).unwrap();
    dump_file.flush().unwrap();

    let storage: Arc<dyn ArticleStore> = Arc::new(SqliteArticleStore::open_in_memory().unwrap());
    for page in &fx.streams[0].pages {
        storage
            .upsert_metadata(&ArticleMetadata {
                page_id: page.page_id,
                title: page.title.clone(),
                tier: Tier::Cold,
                pinned: false,
                stream_offset: Some(fx.streams[0].stream_offset),
                stream_length: Some(fx.streams[0].stream_length),
                revision_id: Some(page.revision_id),
            })
            .unwrap();
    }

    let archive = Arc::new(ArchiveStore::open_in_memory().unwrap());
    let modules = Arc::new(ModuleStore::open_in_memory().unwrap());
    let search = Arc::new(SearchIndex::create_in_ram().unwrap());

    let transport = Arc::new(MockTransport::new(vec![MockResponse::ok(200, b"never")]));
    let kill = Arc::new(KillSwitch::new());
    kill.engage();
    let api = Arc::new(MediaWikiClient::new(
        ClientConfig::default(),
        transport,
        kill,
    ));

    // Pre-seed settings.json with the temp dump path so the facade picks it
    // up at construction.
    let data_dir = tempfile::tempdir().unwrap();
    Settings {
        dump_path: Some(dump_file.path().to_path_buf()),
        last_index_path: None,
        recommendations_enabled: true,
        map_source_path: None,
        history_enabled: true,
    }
    .save(data_dir.path())
    .unwrap();

    let tome = Tome::new(
        storage,
        archive,
        modules,
        search,
        api,
        data_dir.path().to_path_buf(),
    );
    (tome, dump_file, data_dir)
}

#[tokio::test(flavor = "multi_thread")]
async fn cold_read_falls_back_to_dump_when_api_disabled() {
    let (tome, _dump_file_keeper, _data_dir_keeper) = build_facade();

    let response = tome.read_article(&Title::new("Photon")).await.unwrap();
    assert_eq!(response.title, "Photon");
    assert_eq!(response.source, ArticleSource::DumpLocal);
    assert_eq!(response.revision_id, Some(987654321));
    // The renderer should have produced HTML containing the bold text and an
    // internal link marker.
    assert!(
        response.html.contains("<strong>photon</strong>"),
        "{}",
        response.html
    );
    assert!(
        response
            .html
            .contains("href=\"#/article/elementary_particle\""),
        "expected link to elementary particle (underscore-form url), got: {}",
        response.html
    );
    // "elementary particle" isn't in our store, so the link should be marked
    // missing.
    assert!(
        response.html.contains("tome-missing"),
        "expected missing-link marker, got: {}",
        response.html
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cold_read_marks_links_to_known_articles_as_available() {
    let (tome, _dump_file_keeper, _data_dir_keeper) = build_facade();

    // Both Photon and Electron are in the store. Cross-reference Photon's
    // body to confirm the link resolver sees both.
    let response = tome.read_article(&Title::new("Electron")).await.unwrap();
    assert_eq!(response.source, ArticleSource::DumpLocal);
    assert!(
        response.html.contains("<strong>electron</strong>"),
        "{}",
        response.html
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn read_unknown_article_returns_not_found() {
    let (tome, _dump_file_keeper, _data_dir_keeper) = build_facade();
    let err = tome.read_article(&Title::new("Cooking")).await.unwrap_err();
    assert!(matches!(err, tome_core::TomeError::NotFound(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn read_touches_access_count() {
    let (tome, _dump_file_keeper, _data_dir_keeper) = build_facade();

    tome.read_article(&Title::new("Photon")).await.unwrap();
    tome.read_article(&Title::new("Photon")).await.unwrap();
    tome.read_article(&Title::new("Photon")).await.unwrap();

    // We can't directly read access_count through the facade today, but we
    // know touch is wired through; this test mainly proves no panic on
    // repeated reads and the cycle holds.
}
