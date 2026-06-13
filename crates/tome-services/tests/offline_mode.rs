//! Offline mode is the load-bearing promise for Tome's canonical user (a
//! fully-disconnected machine). These tests pin the contract: the persisted
//! flag and the in-memory kill switch stay in lockstep, and an offline
//! machine comes back up offline.

use std::sync::Arc;

use tome_api::testing::MockTransport;
use tome_api::{ClientConfig, KillSwitch, MediaWikiClient};
use tome_modules::ModuleStore;
use tome_search::Index as SearchIndex;
use tome_services::Tome;
use tome_storage::{ArchiveStore, ArticleStore, SqliteArticleStore};

/// A Tome over fresh in-memory stores but a REAL on-disk data dir, so
/// settings persist and a second instance can be constructed over the same
/// dir to simulate a restart.
fn tome_in(dir: &std::path::Path) -> Arc<Tome> {
    let storage: Arc<dyn ArticleStore> = Arc::new(SqliteArticleStore::open_in_memory().unwrap());
    let archive = Arc::new(ArchiveStore::open_in_memory().unwrap());
    let modules = Arc::new(ModuleStore::open_in_memory().unwrap());
    let search = Arc::new(SearchIndex::create_in_ram().unwrap());
    let api = Arc::new(MediaWikiClient::new(
        ClientConfig::default(),
        Arc::new(MockTransport::new(vec![])),
        Arc::new(KillSwitch::new()),
    ));
    Arc::new(Tome::new(
        storage,
        archive,
        modules,
        search,
        api,
        dir.to_path_buf(),
    ))
}

#[test]
fn offline_mode_toggles_kill_switch_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let tome = tome_in(dir.path());

    // Default: online, kill switch disengaged.
    assert!(!tome.offline_mode());
    assert!(!tome.kill_switch_engaged());

    // Turning it on engages the kill switch AND persists the flag.
    tome.set_offline_mode(true).unwrap();
    assert!(tome.offline_mode());
    assert!(tome.kill_switch_engaged(), "offline mode must engage the kill switch");

    // Turning it off reverses both.
    tome.set_offline_mode(false).unwrap();
    assert!(!tome.offline_mode());
    assert!(!tome.kill_switch_engaged());
}

#[test]
fn offline_machine_comes_back_up_offline() {
    let dir = tempfile::tempdir().unwrap();
    {
        let tome = tome_in(dir.path());
        tome.set_offline_mode(true).unwrap();
    }

    // Simulate a restart: a fresh Tome over the same data dir. The kill
    // switch is in-memory so it starts disengaged — until startup re-applies
    // the persisted flag.
    let restarted = tome_in(dir.path());
    assert!(restarted.offline_mode(), "the persisted flag survives restart");
    assert!(
        !restarted.kill_switch_engaged(),
        "kill switch is in-memory; not engaged until startup applies the flag"
    );

    restarted.apply_offline_mode_on_startup();
    assert!(
        restarted.kill_switch_engaged(),
        "startup must re-engage offline machines without a re-toggle"
    );
}

#[test]
fn chat_model_path_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let tome = tome_in(dir.path());

    assert!(tome.chat_model_path().is_none());
    let p = std::path::PathBuf::from("/media/usb/phi-4-mini-instruct-Q4_K_M.gguf");
    tome.set_chat_model_path(Some(p.clone())).unwrap();
    assert_eq!(tome.chat_model_path(), Some(p));

    // Persists across restart.
    let restarted = tome_in(dir.path());
    assert!(restarted.chat_model_path().is_some());

    // Clearing falls back to the downloader.
    restarted.set_chat_model_path(None).unwrap();
    assert!(restarted.chat_model_path().is_none());
}

#[test]
fn cancel_ingest_is_safe_when_nothing_is_running() {
    // The flag is reset at the start of each operation, so a stray cancel
    // with no ingest in flight must be a harmless no-op.
    let dir = tempfile::tempdir().unwrap();
    let tome = tome_in(dir.path());
    tome.cancel_ingest();
    tome.cancel_ingest();
    // Nothing to assert beyond "did not panic"; the next ingest would reset
    // the flag via begin_cancelable before reading it.
}
