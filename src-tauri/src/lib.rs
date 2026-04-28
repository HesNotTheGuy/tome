//! Tauri shell.
//!
//! Builds the [`Tome`] facade once at startup, manages it as Tauri state, and
//! exposes a thin layer of `#[tauri::command]` handlers that the React
//! frontend invokes through `@tauri-apps/api/core`.
//!
//! Long-running calls return `Result<T, String>` because Tauri serializes
//! errors as plain strings to the frontend. The string is the
//! `Display` form of [`tome_core::TomeError`].

use std::sync::Arc;

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, State};
use tome_api::{ClientConfig, KillSwitch, MediaWikiClient, ReqwestTransport};
use tome_archive::{ArchiveStore, SavedRevisionMeta};
use tome_core::{SearchHit, Tier, Title};
use tome_dump::DumpReader;
use tome_modules::{InstalledModule, ModuleSpec, ModuleStore};
use tome_search::Index as SearchIndex;
use tome_services::{ArticleResponse, IngestSummary, TierCounts, Tome};
use tome_storage::{ArticleStore, SqliteArticleStore};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tome=debug")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let tome = build_tome(app)?;
            app.manage(Arc::new(tome));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_article,
            search,
            list_modules,
            install_module,
            uninstall_module,
            list_archive,
            search_archive,
            save_revision,
            kill_switch_engaged,
            set_kill_switch,
            breaker_open,
            user_agent,
            tier_counts,
            ingest_index,
            health_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_tome(app: &tauri::App) -> Result<Tome, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let search_dir = data_dir.join("search");
    std::fs::create_dir_all(&search_dir)?;

    let storage: Arc<dyn ArticleStore> =
        Arc::new(SqliteArticleStore::open(&data_dir.join("articles.sqlite"))?);
    let archive = Arc::new(ArchiveStore::open(&data_dir.join("archive.sqlite"))?);
    let modules = Arc::new(ModuleStore::open(&data_dir.join("modules.sqlite"))?);
    let search = Arc::new(SearchIndex::open_dir(&search_dir)?);

    let kill_switch = Arc::new(KillSwitch::new());
    let transport =
        Arc::new(ReqwestTransport::new().map_err(|e| format!("build http transport: {e}"))?);
    let api = Arc::new(MediaWikiClient::new(
        ClientConfig::default(),
        transport,
        kill_switch,
    ));

    // Placeholder dump path — Cold reads will error until the user points at
    // a real multistream dump via the (forthcoming) settings UI.
    let dump_path = data_dir.join("dump.xml.bz2");
    let dump = Arc::new(DumpReader::open(&dump_path));

    Ok(Tome::new(storage, archive, modules, search, api, dump))
}

// --- Command handlers ---------------------------------------------------------
//
// Every command is a thin wrapper that converts to/from the public Rust types
// and stringifies errors. The Tome facade owns the actual logic.

#[tauri::command]
async fn read_article(
    title: String,
    state: State<'_, Arc<Tome>>,
) -> Result<ArticleResponse, String> {
    state
        .read_article(&Title::new(&title))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn search(
    query: String,
    limit: usize,
    tier_filter: Vec<Tier>,
    state: State<'_, Arc<Tome>>,
) -> Result<Vec<SearchHit>, String> {
    state
        .search(&query, limit, &tier_filter)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_modules(state: State<'_, Arc<Tome>>) -> Result<Vec<InstalledModule>, String> {
    state.list_modules().map_err(|e| e.to_string())
}

#[tauri::command]
async fn install_module(
    spec: ModuleSpec,
    members: Vec<String>,
    state: State<'_, Arc<Tome>>,
) -> Result<InstalledModule, String> {
    state
        .install_module(&spec, &members)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn uninstall_module(id: String, state: State<'_, Arc<Tome>>) -> Result<(), String> {
    state.uninstall_module(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_archive(state: State<'_, Arc<Tome>>) -> Result<Vec<SavedRevisionMeta>, String> {
    state.list_archive().map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_archive(
    query: String,
    limit: usize,
    state: State<'_, Arc<Tome>>,
) -> Result<Vec<SavedRevisionMeta>, String> {
    state
        .search_archive(&query, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn save_revision(
    title: String,
    revision_id: u64,
    wikitext: String,
    html: Option<String>,
    user_note: Option<String>,
    state: State<'_, Arc<Tome>>,
) -> Result<i64, String> {
    state
        .save_revision(
            &title,
            revision_id,
            &wikitext,
            html.as_deref(),
            user_note.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn kill_switch_engaged(state: State<'_, Arc<Tome>>) -> bool {
    state.kill_switch_engaged()
}

#[tauri::command]
fn set_kill_switch(engaged: bool, state: State<'_, Arc<Tome>>) {
    state.set_kill_switch(engaged);
}

#[tauri::command]
fn breaker_open(state: State<'_, Arc<Tome>>) -> bool {
    state.breaker_open()
}

#[tauri::command]
fn user_agent(state: State<'_, Arc<Tome>>) -> String {
    state.user_agent().to_string()
}

#[tauri::command]
fn tier_counts(state: State<'_, Arc<Tome>>) -> Result<TierCounts, String> {
    state.tier_counts().map_err(|e| e.to_string())
}

#[tauri::command]
async fn ingest_index(
    path: String,
    app: AppHandle,
    state: State<'_, Arc<Tome>>,
) -> Result<IngestSummary, String> {
    let path = PathBuf::from(path);
    let tome = state.inner().clone();
    // The ingest is sync (SQLite + bz2 stream), but we want to keep the
    // async runtime responsive. spawn_blocking moves it off the runtime
    // thread and gives us a JoinHandle we can await.
    tokio::task::spawn_blocking(move || {
        tome.ingest_index(&path, |count| {
            let _ = app.emit("ingest:progress", count);
        })
    })
    .await
    .map_err(|e| format!("ingest task join: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn health_check() -> &'static str {
    "ok"
}
