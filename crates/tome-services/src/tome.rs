//! The `Tome` facade — the only public surface the UI depends on.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tome_api::{MediaWikiClient, Revision};
use tome_archive::ArchiveStore;
use tome_config::Settings;
use tome_core::{Result, SearchHit, Tier, Title, TomeError};
use tome_dump::{DumpReader, IndexReader, parse_pages};
use tome_modules::{InstalledModule, ModuleSpec, ModuleStore};
use tome_search::Index as SearchIndex;
use tome_storage::{
    ArticleContent, ArticleStore, CategoryLink, CategoryMember, CategoryMemberKind, Geotag,
    MappedGeotag, Redirect, RelatedArticle,
};
use tome_wikitext::Renderer;

use crate::link_resolver::StorageLinkResolver;

const INGEST_BATCH_SIZE: usize = 5_000;
const INGEST_PROGRESS_INTERVAL: u64 = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArticleSource {
    /// Served from the Hot tier — plain wikitext rendered locally.
    HotLocal,
    /// Served from the Warm tier — decompressed wikitext rendered locally.
    WarmLocal,
    /// Served from the dump — decoded stream + local render.
    DumpLocal,
    /// Served from cached Parsoid HTML via the API client.
    ApiCachedHtml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleResponse {
    pub title: String,
    pub html: String,
    pub source: ArticleSource,
    pub revision_id: Option<u64>,
}

pub struct Tome {
    storage: Arc<dyn ArticleStore>,
    archive: Arc<ArchiveStore>,
    modules: Arc<ModuleStore>,
    search: Arc<SearchIndex>,
    api: Arc<MediaWikiClient>,
    /// Where the user has pointed Tome for the multistream dump.
    /// `None` means the user hasn't configured one yet — Cold reads error
    /// with a helpful message rather than failing on a stale path.
    dump_path: Arc<RwLock<Option<PathBuf>>>,
    /// Where settings.json lives. Reading is via [`Settings::load`], writing
    /// happens whenever a path changes so we never lose configuration on
    /// crash.
    data_dir: PathBuf,
    /// Whether to attempt API-cached Parsoid HTML for Cold reads. When
    /// false (or when the API call fails), we render locally from the dump.
    prefer_api_for_cold: bool,
}

impl Tome {
    pub fn new(
        storage: Arc<dyn ArticleStore>,
        archive: Arc<ArchiveStore>,
        modules: Arc<ModuleStore>,
        search: Arc<SearchIndex>,
        api: Arc<MediaWikiClient>,
        data_dir: PathBuf,
    ) -> Self {
        let settings = Settings::load(&data_dir);
        Self {
            storage,
            archive,
            modules,
            search,
            api,
            dump_path: Arc::new(RwLock::new(settings.dump_path)),
            data_dir,
            prefer_api_for_cold: true,
        }
    }

    pub fn with_prefer_api_for_cold(mut self, prefer: bool) -> Self {
        self.prefer_api_for_cold = prefer;
        self
    }

    fn settings(&self) -> Settings {
        Settings::load(&self.data_dir)
    }

    fn save_settings(&self, mutator: impl FnOnce(&mut Settings)) -> Result<()> {
        let mut s = self.settings();
        mutator(&mut s);
        s.save(&self.data_dir)
            .map_err(|e| TomeError::Other(format!("save settings: {e}")))
    }

    fn renderer(&self) -> Renderer {
        Renderer::new(Box::new(StorageLinkResolver::new(self.storage.clone())))
    }

    /// Read an article. Resolves through tier:
    /// - Hot/Warm: decompress (if needed) + local render
    /// - Cold: try cached Parsoid HTML via the API; on failure, decode from
    ///   the dump and render locally
    /// - Evicted: error — caller must confirm before fetching
    /// - Unknown: try the API directly. This is the "read any article
    ///   without having ingested a dump" path — useful for browsing online
    ///   and for letting first-time users try the app before they download
    ///   25+ GB.
    pub async fn read_article(&self, title: &Title) -> Result<ArticleResponse> {
        // First, follow any redirect we know about. Typing "USA" should land
        // the reader on "United States" without round-tripping through the
        // API. The redirect record is keyed off the source page existing in
        // storage, so this only triggers for ingested-and-known redirects.
        let resolved = match self.storage.resolve_redirect(title)? {
            Some(target) => Title::new(&target),
            None => title.clone(),
        };

        let record = match self.storage.lookup(&resolved)? {
            Some(r) => r,
            None => {
                if !self.prefer_api_for_cold {
                    return Err(TomeError::NotFound(title.to_string()));
                }
                // Not in our store — fetch latest from the API. This works
                // regardless of dump configuration. If the article doesn't
                // exist on Wikipedia, fetch_html surfaces an Api error which
                // we map to NotFound so the UI can render the same empty
                // state for both cases.
                let html = self
                    .api
                    .fetch_html(title.as_str(), None)
                    .await
                    .map_err(|e| TomeError::NotFound(format!("{title}: {e}")))?;
                return Ok(ArticleResponse {
                    title: title.to_string(),
                    html,
                    source: ArticleSource::ApiCachedHtml,
                    revision_id: None,
                });
            }
        };

        let content = self
            .storage
            .get_content(record.metadata.page_id)?
            .ok_or_else(|| TomeError::NotFound(title.to_string()))?;

        let response = match content {
            ArticleContent::Hot(wikitext) => {
                let html = self.renderer().render(&wikitext);
                ArticleResponse {
                    title: record.metadata.title.clone(),
                    html,
                    source: ArticleSource::HotLocal,
                    revision_id: record.metadata.revision_id,
                }
            }
            ArticleContent::Warm(wikitext) => {
                let html = self.renderer().render(&wikitext);
                ArticleResponse {
                    title: record.metadata.title.clone(),
                    html,
                    source: ArticleSource::WarmLocal,
                    revision_id: record.metadata.revision_id,
                }
            }
            ArticleContent::Cold {
                stream_offset,
                stream_length,
            } => {
                self.resolve_cold(
                    &record.metadata.title,
                    record.metadata.page_id,
                    record.metadata.revision_id,
                    stream_offset,
                    stream_length,
                )
                .await?
            }
            ArticleContent::Evicted => {
                return Err(TomeError::Other(format!(
                    "article '{}' is evicted; user confirmation required to fetch",
                    record.metadata.title
                )));
            }
        };

        self.storage.touch(record.metadata.page_id)?;
        Ok(response)
    }

    async fn resolve_cold(
        &self,
        title: &str,
        page_id: u64,
        revision_id: Option<u64>,
        stream_offset: u64,
        stream_length: Option<u64>,
    ) -> Result<ArticleResponse> {
        if self.prefer_api_for_cold {
            match self.api.fetch_html(title, revision_id).await {
                Ok(html) => {
                    return Ok(ArticleResponse {
                        title: title.to_string(),
                        html,
                        source: ArticleSource::ApiCachedHtml,
                        revision_id,
                    });
                }
                Err(_) => {
                    // Fall through to local render — offline, kill switch
                    // engaged, breaker open, or any other failure.
                }
            }
        }
        let dump_path = self
            .dump_path
            .read()
            .map_err(|e| TomeError::Other(format!("dump path lock poisoned: {e}")))?
            .clone()
            .ok_or_else(|| {
                TomeError::Other(
                    "dump path not configured — set it in Settings before reading Cold articles"
                        .to_string(),
                )
            })?;
        let dump = DumpReader::open(&dump_path);
        let bytes = dump.read_stream(stream_offset, stream_length)?;
        let pages = parse_pages(&bytes)?;
        let page = pages.iter().find(|p| p.page_id == page_id).ok_or_else(|| {
            TomeError::Dump(format!(
                "page_id {page_id} not found in stream at {stream_offset}"
            ))
        })?;
        let html = self.renderer().render(&page.wikitext);
        Ok(ArticleResponse {
            title: title.to_string(),
            html,
            source: ArticleSource::DumpLocal,
            revision_id: Some(page.revision_id),
        })
    }

    pub fn search(
        &self,
        query: &str,
        limit: usize,
        tier_filter: &[Tier],
    ) -> Result<Vec<SearchHit>> {
        self.search.search(query, limit, tier_filter)
    }

    /// Install a module given pre-resolved member titles. Category resolution
    /// happens in a separate step (caller composes the resolver against the
    /// API client). This split keeps the install operation transactional and
    /// testable without a network.
    pub fn install_module(
        &self,
        spec: &ModuleSpec,
        resolved_members: &[String],
    ) -> Result<InstalledModule> {
        spec.validate()?;
        self.modules.install(spec, resolved_members)?;
        self.modules
            .get(&spec.id)?
            .ok_or_else(|| TomeError::Other("install succeeded but module not found".into()))
    }

    pub fn uninstall_module(&self, id: &str) -> Result<()> {
        self.modules.uninstall(id)
    }

    pub fn list_modules(&self) -> Result<Vec<InstalledModule>> {
        self.modules.list()
    }

    /// Save a revision permanently. The caller provides the content; fetching
    /// from the API is composed at the UI/services layer once the revision
    /// API surface lands.
    pub fn save_revision(
        &self,
        title: &str,
        revision_id: u64,
        wikitext: &str,
        html: Option<&str>,
        user_note: Option<&str>,
    ) -> Result<i64> {
        self.archive
            .save(title, revision_id, wikitext, html, user_note)
    }

    pub fn list_archive(&self) -> Result<Vec<tome_archive::SavedRevisionMeta>> {
        self.archive.list()
    }

    pub fn search_archive(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<tome_archive::SavedRevisionMeta>> {
        self.archive.search(query, limit)
    }

    /// Fetch the most recent revisions for an article from the MediaWiki
    /// action API. Used by the Reader timeline. Capped at 500 by the API.
    pub async fn fetch_revisions(&self, title: &str, limit: u32) -> Result<Vec<Revision>> {
        self.api.fetch_revisions(title, limit).await
    }

    /// Read a TOML module file from `path`, parse it, validate, and install
    /// using the spec's `explicit_titles` as members. Category resolution
    /// (calling the MediaWiki API to expand categories into title lists) is
    /// a follow-up — for now, modules are pure title-list bundles.
    pub fn import_module_from_path(&self, path: &Path) -> Result<InstalledModule> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| TomeError::Other(format!("read module file {path:?}: {e}")))?;
        let spec = ModuleSpec::from_toml(&text)?;
        spec.validate()?;
        // For v1 we install with whatever explicit_titles the user listed.
        // Category resolution will fill in titles from Wikipedia categories
        // once the API surface is wired up.
        self.modules.install(&spec, &spec.explicit_titles)?;
        self.modules
            .get(&spec.id)?
            .ok_or_else(|| TomeError::Other("install succeeded but module not found".into()))
    }

    // --- Settings / introspection ---

    pub fn kill_switch_engaged(&self) -> bool {
        self.api.kill_switch().is_engaged()
    }

    pub fn set_kill_switch(&self, engaged: bool) {
        if engaged {
            self.api.kill_switch().engage();
        } else {
            self.api.kill_switch().disengage();
        }
    }

    pub fn breaker_open(&self) -> bool {
        self.api.breaker_is_open()
    }

    pub fn user_agent(&self) -> &str {
        tome_config::DEFAULT_USER_AGENT
    }

    /// Current dump file path (if configured).
    pub fn dump_path(&self) -> Option<PathBuf> {
        self.dump_path.read().ok().and_then(|g| g.clone())
    }

    /// Configure (or clear) the dump file path. Persisted to settings.json
    /// immediately so the value survives the next launch.
    pub fn set_dump_path(&self, path: Option<PathBuf>) -> Result<()> {
        *self
            .dump_path
            .write()
            .map_err(|e| TomeError::Other(format!("dump path lock poisoned: {e}")))? = path.clone();
        self.save_settings(|s| s.dump_path = path)
    }

    /// The last index path the user ingested, if any. Used by the UI to
    /// pre-fill the ingest input on subsequent launches.
    pub fn last_index_path(&self) -> Option<PathBuf> {
        self.settings().last_index_path
    }

    /// User-configured path to a `.pmtiles` archive used by the Map pane as
    /// an offline basemap. `None` means the Map falls back to live OSM.
    pub fn map_source_path(&self) -> Option<PathBuf> {
        self.settings().map_source_path
    }

    /// Configure (or clear) the offline map source. Persisted immediately.
    pub fn set_map_source_path(&self, path: Option<PathBuf>) -> Result<()> {
        self.save_settings(|s| s.map_source_path = path)
    }

    pub fn tier_counts(&self) -> Result<TierCounts> {
        Ok(TierCounts {
            hot: self.storage.count_by_tier(Tier::Hot)?,
            warm: self.storage.count_by_tier(Tier::Warm)?,
            cold: self.storage.count_by_tier(Tier::Cold)?,
            evicted: self.storage.count_by_tier(Tier::Evicted)?,
        })
    }

    /// Stream-parse a Wikipedia multistream index file (`*-multistream-index.txt.bz2`)
    /// and upsert every entry as Cold-tier metadata. The full index is
    /// ~6.5M lines for English Wikipedia; this typically completes in 30-90s
    /// on an SSD.
    ///
    /// `on_progress` is called every ~10K entries with the running count, so
    /// the UI can show a live counter without an event channel.
    pub fn ingest_index<F>(&self, index_path: &Path, mut on_progress: F) -> Result<IngestSummary>
    where
        F: FnMut(u64),
    {
        let started = Instant::now();
        let file = std::fs::File::open(index_path)
            .map_err(|e| TomeError::Dump(format!("open index {index_path:?}: {e}")))?;
        let reader = IndexReader::new(file);

        let mut batch: Vec<(u64, String, u64)> = Vec::with_capacity(INGEST_BATCH_SIZE);
        let mut total: u64 = 0;
        let mut next_progress = INGEST_PROGRESS_INTERVAL;

        for entry in reader {
            let entry = entry?;
            batch.push((entry.page_id, entry.title, entry.stream_offset));
            if batch.len() >= INGEST_BATCH_SIZE {
                let n = self.storage.batch_upsert_cold(&batch)?;
                total += n;
                batch.clear();
                if total >= next_progress {
                    on_progress(total);
                    next_progress = total + INGEST_PROGRESS_INTERVAL;
                }
            }
        }
        if !batch.is_empty() {
            let n = self.storage.batch_upsert_cold(&batch)?;
            total += n;
        }
        on_progress(total);

        // Remember the index path so the UI can pre-fill it next launch.
        let _ = self.save_settings(|s| s.last_index_path = Some(index_path.to_path_buf()));

        Ok(IngestSummary {
            entries_processed: total,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// Stream-parse a Wikipedia `geo_tags.sql.gz` and bulk-insert each row
    /// as a `geotags` record. Tiny relative to the article dump (~50 MB
    /// compressed for enwiki, low MB for simplewiki) so we collect into
    /// memory and batch upsert.
    pub fn ingest_geotags<F>(&self, path: &Path, mut on_progress: F) -> Result<GeotagSummary>
    where
        F: FnMut(u64),
    {
        let started = Instant::now();
        let mut all: Vec<Geotag> = Vec::new();
        crate::geotag_ingest::parse_file(path, |g| all.push(g))?;

        let mut total = 0_u64;
        let mut next_progress = 10_000_u64;
        for chunk in all.chunks(5_000) {
            total += self.storage.batch_upsert_geotags(chunk)?;
            if total >= next_progress {
                on_progress(total);
                next_progress = total + 10_000;
            }
        }
        on_progress(total);

        Ok(GeotagSummary {
            entries_processed: total,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// Look up the geographic coordinate for an article by title. Returns
    /// `None` if the article has no geotag, or if the article isn't in
    /// storage at all (e.g., user is reading via the API fallback path
    /// before ingestion).
    pub fn geotag_for_title(&self, title: &Title) -> Result<Option<Geotag>> {
        let Some(rec) = self.storage.lookup(title)? else {
            return Ok(None);
        };
        self.storage.geotag_for(rec.metadata.page_id)
    }

    pub fn count_geotags(&self) -> Result<u64> {
        self.storage.count_geotags()
    }

    /// Every primary geotag whose article we've indexed, joined with its
    /// title. Powers the Map pane.
    pub fn all_primary_geotags(&self) -> Result<Vec<MappedGeotag>> {
        self.storage.all_primary_geotags()
    }

    /// Stream-parse a Wikipedia `categorylinks.sql.gz` and bulk-insert each
    /// row. Larger than geotags (~28 MB simplewiki, ~2.4 GB enwiki
    /// compressed; ~5M rows simplewiki, ~250M enwiki). For simplewiki this
    /// finishes in seconds; for enwiki, several minutes.
    pub fn ingest_categorylinks<F>(
        &self,
        path: &Path,
        mut on_progress: F,
    ) -> Result<CategoryIngestSummary>
    where
        F: FnMut(u64),
    {
        let started = Instant::now();
        let mut buffer: Vec<CategoryLink> = Vec::with_capacity(INGEST_BATCH_SIZE);
        let mut total = 0_u64;
        let mut next_progress = INGEST_PROGRESS_INTERVAL;
        let storage = self.storage.clone();
        // Errors that occur inside the parse callback can't propagate
        // through `parse_file` directly (its callback type is FnMut without
        // a Result return). We capture into this Option and check after.
        let mut callback_err: Option<TomeError> = None;

        crate::category_ingest::parse_file(path, |link| {
            if callback_err.is_some() {
                return;
            }
            buffer.push(link);
            if buffer.len() >= INGEST_BATCH_SIZE {
                match storage.batch_upsert_categorylinks(&buffer) {
                    Ok(n) => {
                        total += n;
                        buffer.clear();
                        if total >= next_progress {
                            on_progress(total);
                            next_progress = total + INGEST_PROGRESS_INTERVAL;
                        }
                    }
                    Err(e) => callback_err = Some(e),
                }
            }
        })?;
        if let Some(e) = callback_err {
            return Err(e);
        }
        if !buffer.is_empty() {
            total += storage.batch_upsert_categorylinks(&buffer)?;
        }
        on_progress(total);

        Ok(CategoryIngestSummary {
            entries_processed: total,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    pub fn category_members(
        &self,
        category: &str,
        kind: Option<CategoryMemberKind>,
        limit: u32,
    ) -> Result<Vec<CategoryMember>> {
        self.storage.category_members(category, kind, limit)
    }

    pub fn categories_for_title(&self, title: &Title) -> Result<Vec<String>> {
        let Some(rec) = self.storage.lookup(title)? else {
            return Ok(Vec::new());
        };
        self.storage.categories_for(rec.metadata.page_id)
    }

    pub fn search_categories(&self, prefix: &str, limit: u32) -> Result<Vec<String>> {
        self.storage.search_categories(prefix, limit)
    }

    pub fn count_categorylinks(&self) -> Result<u64> {
        self.storage.count_categorylinks()
    }

    /// Articles related to the given title via shared categorylinks. Empty
    /// vec if recommendations are disabled in settings, the article isn't
    /// in our store, or no categorylinks have been ingested.
    pub fn related_to_title(&self, title: &Title, limit: u32) -> Result<Vec<RelatedArticle>> {
        if !self.settings().recommendations_enabled {
            return Ok(Vec::new());
        }
        let Some(rec) = self.storage.lookup(title)? else {
            return Ok(Vec::new());
        };
        self.storage.related_to(rec.metadata.page_id, limit)
    }

    pub fn recommendations_enabled(&self) -> bool {
        self.settings().recommendations_enabled
    }

    pub fn set_recommendations_enabled(&self, enabled: bool) -> Result<()> {
        self.save_settings(|s| s.recommendations_enabled = enabled)
    }

    /// Stream-parse a Wikipedia `redirect.sql.gz` and bulk-insert each
    /// row. Small relative to articles (~1 MB simple, ~250 MB enwiki).
    pub fn ingest_redirects<F>(
        &self,
        path: &Path,
        mut on_progress: F,
    ) -> Result<RedirectIngestSummary>
    where
        F: FnMut(u64),
    {
        let started = Instant::now();
        let mut buffer: Vec<Redirect> = Vec::with_capacity(INGEST_BATCH_SIZE);
        let mut total = 0_u64;
        let mut next_progress = INGEST_PROGRESS_INTERVAL;
        let storage = self.storage.clone();
        let mut callback_err: Option<TomeError> = None;

        crate::redirect_ingest::parse_file(path, |r| {
            if callback_err.is_some() {
                return;
            }
            buffer.push(r);
            if buffer.len() >= INGEST_BATCH_SIZE {
                match storage.batch_upsert_redirects(&buffer) {
                    Ok(n) => {
                        total += n;
                        buffer.clear();
                        if total >= next_progress {
                            on_progress(total);
                            next_progress = total + INGEST_PROGRESS_INTERVAL;
                        }
                    }
                    Err(e) => callback_err = Some(e),
                }
            }
        })?;
        if let Some(e) = callback_err {
            return Err(e);
        }
        if !buffer.is_empty() {
            total += storage.batch_upsert_redirects(&buffer)?;
        }
        on_progress(total);

        Ok(RedirectIngestSummary {
            entries_processed: total,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    pub fn resolve_redirect(&self, source: &Title) -> Result<Option<String>> {
        self.storage.resolve_redirect(source)
    }

    pub fn count_redirects(&self) -> Result<u64> {
        self.storage.count_redirects()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedirectIngestSummary {
    pub entries_processed: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeotagSummary {
    pub entries_processed: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryIngestSummary {
    pub entries_processed: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSummary {
    pub entries_processed: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierCounts {
    pub hot: u64,
    pub warm: u64,
    pub cold: u64,
    pub evicted: u64,
}
