//! Tauri shell.
//!
//! Builds the [`Tome`] facade once at startup, manages it as Tauri state, and
//! exposes a thin layer of `#[tauri::command]` handlers that the React
//! frontend invokes through `@tauri-apps/api/core`.
//!
//! Long-running calls return `Result<T, String>` because Tauri serializes
//! errors as plain strings to the frontend. The string is the
//! `Display` form of [`tome_core::TomeError`].

mod nav_guard;
mod pmtiles_protocol;

use std::sync::Arc;

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, State};
use tome_api::{ClientConfig, KillSwitch, MediaWikiClient, ReqwestTransport, Revision};
use tome_archive::{ArchiveStore, SavedRevisionMeta};
use tome_core::{SearchHit, Tier, Title};
use tome_modules::{InstalledModule, ModuleSpec, ModuleStore};
use tome_search::Index as SearchIndex;
use tome_services::{
    ArticleResponse, CategoryIngestSummary, EmbeddingIngestSummary, GeotagSummary, IngestSummary,
    RedirectIngestSummary, TierCounts, Tome,
};
use tome_storage::{
    ArticleStore, CategoryMember, CategoryMemberKind, EmbeddingHit, Geotag, MappedGeotag,
    RelatedArticle, SqliteArticleStore,
};

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
        .register_asynchronous_uri_scheme_protocol(
            pmtiles_protocol::SCHEME,
            pmtiles_protocol::handle,
        )
        .setup(|app| {
            let tome = build_tome(app)?;
            app.manage(Arc::new(tome));
            build_main_window(app)?;
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
            ingest_geotags,
            count_geotags,
            geotag_for_title,
            all_primary_geotags,
            ingest_categorylinks,
            category_members,
            categories_for_title,
            search_categories,
            count_categorylinks,
            ingest_redirects,
            count_redirects,
            related_to_title,
            recommendations_enabled,
            set_recommendations_enabled,
            fetch_revisions,
            import_module_from_path,
            dump_path,
            set_dump_path,
            last_index_path,
            map_source_path,
            set_map_source_path,
            embed_articles,
            count_embeddings,
            semantic_search,
            chat_model_present,
            download_chat_model,
            health_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Construct the main webview window with the navigation guard attached.
///
/// We build the window in Rust setup rather than declaring it in
/// `tauri.conf.json` because [`tauri::WebviewWindowBuilder::on_navigation`]
/// is only available before `build()` — there's no after-the-fact way to
/// attach a navigation listener to a window Tauri creates from config.
fn build_main_window(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title("Tome")
        .inner_size(1280.0, 800.0)
        .min_inner_size(800.0, 500.0)
        .resizable(true)
        .fullscreen(false)
        .on_navigation(move |url| {
            if nav_guard::is_internal_url(url) {
                return true;
            }
            nav_guard::open_external(&handle, url);
            false
        })
        .build()?;
    Ok(())
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

    Ok(Tome::new(storage, archive, modules, search, api, data_dir))
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
async fn fetch_revisions(
    title: String,
    limit: u32,
    state: State<'_, Arc<Tome>>,
) -> Result<Vec<Revision>, String> {
    state
        .fetch_revisions(&title, limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn import_module_from_path(
    path: String,
    state: State<'_, Arc<Tome>>,
) -> Result<InstalledModule, String> {
    let path = PathBuf::from(path);
    state
        .import_module_from_path(&path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn dump_path(state: State<'_, Arc<Tome>>) -> Option<String> {
    state.dump_path().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn set_dump_path(path: Option<String>, state: State<'_, Arc<Tome>>) -> Result<(), String> {
    let pb = path.map(PathBuf::from);
    state.set_dump_path(pb).map_err(|e| e.to_string())
}

#[tauri::command]
fn last_index_path(state: State<'_, Arc<Tome>>) -> Option<String> {
    state
        .last_index_path()
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn map_source_path(state: State<'_, Arc<Tome>>) -> Option<String> {
    state
        .map_source_path()
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
fn set_map_source_path(path: Option<String>, state: State<'_, Arc<Tome>>) -> Result<(), String> {
    let pb = path.map(PathBuf::from);
    state.set_map_source_path(pb).map_err(|e| e.to_string())
}

#[tauri::command]
async fn embed_articles(
    max_articles: u64,
    app: AppHandle,
    state: State<'_, Arc<Tome>>,
) -> Result<EmbeddingIngestSummary, String> {
    let tome = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        tome.embed_articles(max_articles, |count| {
            let _ = app.emit("ai:embedding_progress", count);
        })
    })
    .await
    .map_err(|e| format!("embedding task join: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn count_embeddings(state: State<'_, Arc<Tome>>) -> Result<u64, String> {
    state.count_embeddings().map_err(|e| e.to_string())
}

#[tauri::command]
async fn semantic_search(
    query: String,
    k: u32,
    state: State<'_, Arc<Tome>>,
) -> Result<Vec<EmbeddingHit>, String> {
    let tome = state.inner().clone();
    // Off the runtime: the embed_one call hits the model, the cosine scan
    // walks every stored vector. Both are CPU-bound; spawn_blocking keeps
    // the IPC reactor responsive.
    tokio::task::spawn_blocking(move || tome.semantic_search(&query, k))
        .await
        .map_err(|e| format!("semantic search task join: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn chat_model_present(state: State<'_, Arc<Tome>>) -> bool {
    state.chat_model_present()
}

#[tauri::command]
async fn download_chat_model(
    app: AppHandle,
    state: State<'_, Arc<Tome>>,
) -> Result<String, String> {
    let tome = state.inner().clone();
    let path = tome
        .download_chat_model(move |bytes| {
            let _ = app.emit("ai:chat_download_progress", bytes);
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn ingest_geotags(
    path: String,
    app: AppHandle,
    state: State<'_, Arc<Tome>>,
) -> Result<GeotagSummary, String> {
    let path = PathBuf::from(path);
    let tome = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        tome.ingest_geotags(&path, |count| {
            let _ = app.emit("geotag:progress", count);
        })
    })
    .await
    .map_err(|e| format!("geotag ingest task join: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn count_geotags(state: State<'_, Arc<Tome>>) -> Result<u64, String> {
    state.count_geotags().map_err(|e| e.to_string())
}

#[tauri::command]
fn geotag_for_title(title: String, state: State<'_, Arc<Tome>>) -> Result<Option<Geotag>, String> {
    state
        .geotag_for_title(&Title::new(&title))
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn all_primary_geotags(state: State<'_, Arc<Tome>>) -> Result<Vec<MappedGeotag>, String> {
    let tome = state.inner().clone();
    tokio::task::spawn_blocking(move || tome.all_primary_geotags())
        .await
        .map_err(|e| format!("all_primary_geotags task join: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ingest_categorylinks(
    path: String,
    app: AppHandle,
    state: State<'_, Arc<Tome>>,
) -> Result<CategoryIngestSummary, String> {
    let path = PathBuf::from(path);
    let tome = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        tome.ingest_categorylinks(&path, |count| {
            let _ = app.emit("categorylinks:progress", count);
        })
    })
    .await
    .map_err(|e| format!("categorylinks ingest task join: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn category_members(
    category: String,
    kind: Option<String>,
    limit: u32,
    state: State<'_, Arc<Tome>>,
) -> Result<Vec<CategoryMember>, String> {
    let kind_filter = kind.as_deref().and_then(CategoryMemberKind::parse);
    state
        .category_members(&category, kind_filter, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn categories_for_title(title: String, state: State<'_, Arc<Tome>>) -> Result<Vec<String>, String> {
    state
        .categories_for_title(&Title::new(&title))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn search_categories(
    prefix: String,
    limit: u32,
    state: State<'_, Arc<Tome>>,
) -> Result<Vec<String>, String> {
    state
        .search_categories(&prefix, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ingest_redirects(
    path: String,
    app: AppHandle,
    state: State<'_, Arc<Tome>>,
) -> Result<RedirectIngestSummary, String> {
    let path = PathBuf::from(path);
    let tome = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        tome.ingest_redirects(&path, |count| {
            let _ = app.emit("redirects:progress", count);
        })
    })
    .await
    .map_err(|e| format!("redirect ingest task join: {e}"))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn count_redirects(state: State<'_, Arc<Tome>>) -> Result<u64, String> {
    state.count_redirects().map_err(|e| e.to_string())
}

#[tauri::command]
fn count_categorylinks(state: State<'_, Arc<Tome>>) -> Result<u64, String> {
    state.count_categorylinks().map_err(|e| e.to_string())
}

#[tauri::command]
fn related_to_title(
    title: String,
    limit: u32,
    state: State<'_, Arc<Tome>>,
) -> Result<Vec<RelatedArticle>, String> {
    state
        .related_to_title(&Title::new(&title), limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn recommendations_enabled(state: State<'_, Arc<Tome>>) -> bool {
    state.recommendations_enabled()
}

#[tauri::command]
fn set_recommendations_enabled(enabled: bool, state: State<'_, Arc<Tome>>) -> Result<(), String> {
    state
        .set_recommendations_enabled(enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn health_check() -> &'static str {
    "ok"
}
