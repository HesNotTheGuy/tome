//! The `Tome` facade — the only public surface the UI depends on.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tome_api::MediaWikiClient;
use tome_archive::ArchiveStore;
use tome_core::{Result, SearchHit, Tier, Title, TomeError};
use tome_dump::{DumpReader, IndexReader, parse_pages};
use tome_modules::{InstalledModule, ModuleSpec, ModuleStore};
use tome_search::Index as SearchIndex;
use tome_storage::{ArticleContent, ArticleStore};
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
    dump: Arc<DumpReader>,
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
        dump: Arc<DumpReader>,
    ) -> Self {
        Self {
            storage,
            archive,
            modules,
            search,
            api,
            dump,
            prefer_api_for_cold: true,
        }
    }

    pub fn with_prefer_api_for_cold(mut self, prefer: bool) -> Self {
        self.prefer_api_for_cold = prefer;
        self
    }

    fn renderer(&self) -> Renderer {
        Renderer::new(Box::new(StorageLinkResolver::new(self.storage.clone())))
    }

    /// Read an article. Resolves through tier:
    /// - Hot/Warm: decompress (if needed) + local render
    /// - Cold: try cached Parsoid HTML via the API; on failure, decode from
    ///   the dump and render locally
    /// - Evicted: error — caller must confirm before fetching
    pub async fn read_article(&self, title: &Title) -> Result<ArticleResponse> {
        let record = self
            .storage
            .lookup(title)?
            .ok_or_else(|| TomeError::NotFound(title.to_string()))?;

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
        let bytes = self.dump.read_stream(stream_offset, stream_length)?;
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

        Ok(IngestSummary {
            entries_processed: total,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }
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
