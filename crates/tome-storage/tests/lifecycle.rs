//! Integration test exercising the full tier lifecycle of an article through a
//! file-backed SqliteArticleStore. Covers persistence across reopens.

use tempfile::tempdir;
use tome_core::{Tier, Title};
use tome_storage::{ArticleContent, ArticleMetadata, ArticleStore, SqliteArticleStore};

fn cold_meta(page_id: u64, title: &str, offset: u64) -> ArticleMetadata {
    ArticleMetadata {
        page_id,
        title: title.into(),
        tier: Tier::Cold,
        pinned: false,
        stream_offset: Some(offset),
        stream_length: Some(2048),
        revision_id: Some(1),
    }
}

#[test]
fn full_tier_round_trip_with_file_backing() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("articles.sqlite");

    {
        let store = SqliteArticleStore::open(&db_path).unwrap();
        store
            .upsert_metadata(&cold_meta(7, "Quantum entanglement", 5_000))
            .unwrap();

        store
            .set_tier(7, Tier::Hot, Some("Hot wikitext for entanglement"))
            .unwrap();
        store.touch(7).unwrap();

        store
            .set_tier(7, Tier::Warm, Some("Warm wikitext for entanglement"))
            .unwrap();
        store.touch(7).unwrap();
    }

    // Reopen the same file: state must persist.
    let store = SqliteArticleStore::open(&db_path).unwrap();
    let rec = store
        .lookup(&Title::new("Quantum entanglement"))
        .unwrap()
        .expect("entanglement persisted");
    assert_eq!(rec.metadata.tier, Tier::Warm);
    assert_eq!(rec.access_count, 2);

    let content = store.get_content(7).unwrap().unwrap();
    let ArticleContent::Warm(text) = content else {
        panic!("expected Warm, got {content:?}");
    };
    assert_eq!(text, "Warm wikitext for entanglement");

    // Demote to Cold. Cached body is dropped; offset still resolves.
    store.set_tier(7, Tier::Cold, None).unwrap();
    let content = store.get_content(7).unwrap().unwrap();
    match content {
        ArticleContent::Cold {
            stream_offset,
            stream_length,
        } => {
            assert_eq!(stream_offset, 5_000);
            assert_eq!(stream_length, Some(2048));
        }
        other => panic!("expected Cold, got {other:?}"),
    }
}

#[test]
fn lru_eviction_simulation() {
    let store = SqliteArticleStore::open_in_memory().unwrap();

    // Seed 5 Hot articles, all touched once.
    for i in 1..=5_u64 {
        store
            .upsert_metadata(&cold_meta(i, &format!("Page{i}"), i * 100))
            .unwrap();
        store.set_tier(i, Tier::Hot, Some("body")).unwrap();
        store.touch(i).unwrap();
    }
    // Touch 4 and 5 several times to mark them "recent".
    for _ in 0..3 {
        store.touch(4).unwrap();
        store.touch(5).unwrap();
    }
    // Pin page 1 — must never appear in candidates regardless of access age.
    store.pin(1, true).unwrap();

    // Demote the 2 LRU candidates to Warm.
    let demote_targets = store.lru_candidates(2).unwrap();
    assert_eq!(demote_targets.len(), 2);
    assert!(!demote_targets.contains(&1), "pinned page must be excluded");
    assert!(
        !demote_targets.contains(&4),
        "recent page must not be candidate"
    );
    assert!(
        !demote_targets.contains(&5),
        "recent page must not be candidate"
    );

    for id in &demote_targets {
        store.set_tier(*id, Tier::Warm, Some("body")).unwrap();
    }

    assert_eq!(store.count_by_tier(Tier::Hot).unwrap(), 3);
    assert_eq!(store.count_by_tier(Tier::Warm).unwrap(), 2);
}
