//! `ArticleStore` trait and SQLite-backed implementation.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use tome_core::{Result, Tier, Title, TomeError};

use crate::article::{ArticleContent, ArticleMetadata, ArticleRecord};
use crate::{compression, schema};

/// Storage abstraction over the tiered article store.
///
/// Implementations are free to use any backing store (a real SQLite file, an
/// in-memory database, a mock for tests). Higher layers depend only on this
/// trait so they can be tested with stand-ins.
pub trait ArticleStore: Send + Sync {
    fn upsert_metadata(&self, m: &ArticleMetadata) -> Result<()>;
    /// Bulk insert/update Cold-tier metadata in a single transaction. Used by
    /// dump ingestion where we need 6M+ rows in under a minute. Returns the
    /// number of rows processed. Per-row writes through `upsert_metadata` are
    /// 100x slower because of per-call lock + transaction overhead.
    fn batch_upsert_cold(&self, entries: &[(u64, String, u64)]) -> Result<u64>;
    fn lookup(&self, title: &Title) -> Result<Option<ArticleRecord>>;
    fn get_content(&self, page_id: u64) -> Result<Option<ArticleContent>>;
    /// Move an article to a new tier.
    ///
    /// - `Hot` and `Warm` require `content`.
    /// - `Cold` and `Evicted` ignore `content`; any previously stored content
    ///   for this page is dropped.
    /// - `Cold` requires the article's metadata to already record
    ///   `stream_offset` (set this via `upsert_metadata` first).
    fn set_tier(&self, page_id: u64, tier: Tier, content: Option<&str>) -> Result<()>;
    fn pin(&self, page_id: u64, pinned: bool) -> Result<()>;
    fn touch(&self, page_id: u64) -> Result<()>;
    fn count_by_tier(&self, tier: Tier) -> Result<u64>;
    /// Up to `n` non-pinned Hot/Warm article ids ordered by least-recently
    /// accessed first. Used by the demotion policy in higher layers.
    fn lru_candidates(&self, n: u32) -> Result<Vec<u64>>;
    /// A uniformly-random article title from any non-evicted tier.
    /// Returns `None` if storage is empty. Powers the "Random article"
    /// button in the header — gives users a way to discover content
    /// without making any editorial choices on our part.
    fn random_article_title(&self) -> Result<Option<String>>;
    /// Recently-read articles ordered by `last_accessed DESC`. Returns
    /// at most `limit` rows. Excludes articles never read (last_accessed = 0).
    /// Powers the History pane.
    fn recent_articles(&self, limit: u32) -> Result<Vec<HistoryEntry>>;
    /// Reset `last_accessed = 0` and `access_count = 0` on every row in
    /// articles. Used by the History pane's "Clear history" button.
    /// Returns the number of rows touched (i.e., previously had access > 0).
    fn clear_history(&self) -> Result<u64>;

    // --- Bookmarks ---

    /// Add a bookmark. Returns the new id. If a bookmark for the same
    /// `(article_title, folder_id)` already exists, returns its id
    /// (idempotent — clicking the bookmark button twice doesn't create
    /// duplicates).
    fn add_bookmark(
        &self,
        article_title: &str,
        folder_id: Option<i64>,
        note: Option<&str>,
    ) -> Result<i64>;
    /// Remove a bookmark by id. Idempotent — removing a missing id is fine.
    fn remove_bookmark(&self, id: i64) -> Result<()>;
    /// Move a bookmark to a different folder (or to root with `None`).
    fn move_bookmark(&self, id: i64, folder_id: Option<i64>) -> Result<()>;
    /// Whether the article is bookmarked anywhere (any folder, including
    /// root). Powers the Reader's bookmark button toggle state.
    fn is_bookmarked(&self, article_title: &str) -> Result<bool>;
    /// All bookmarks under `folder_id` (or all root-level if `None`),
    /// ordered by created_at DESC.
    fn bookmarks_in_folder(
        &self,
        folder_id: Option<i64>,
        limit: u32,
    ) -> Result<Vec<crate::bookmark::Bookmark>>;
    /// Every bookmark across every folder, ordered by created_at DESC.
    fn all_bookmarks(&self, limit: u32) -> Result<Vec<crate::bookmark::Bookmark>>;
    fn count_bookmarks(&self) -> Result<u64>;

    // --- Bookmark folders ---

    /// Create a new folder. Returns the new id. `parent_id = None`
    /// creates a root-level folder.
    fn create_folder(&self, name: &str, parent_id: Option<i64>) -> Result<i64>;
    /// Rename a folder.
    fn rename_folder(&self, id: i64, new_name: &str) -> Result<()>;
    /// Delete a folder. Bookmarks within it become unfiled
    /// (`folder_id = NULL`); subfolders are cascade-deleted.
    fn delete_folder(&self, id: i64) -> Result<()>;
    /// All folders, ordered by name.
    fn list_folders(&self) -> Result<Vec<crate::bookmark::BookmarkFolder>>;

    // --- Geotags ---

    /// Bulk insert/update geotag rows in a single transaction. Returns rows
    /// processed.
    fn batch_upsert_geotags(&self, entries: &[crate::geotag::Geotag]) -> Result<u64>;
    /// Primary geotag for an article, if any.
    fn geotag_for(&self, page_id: u64) -> Result<Option<crate::geotag::Geotag>>;
    fn count_geotags(&self) -> Result<u64>;
    /// Every primary geotag whose article we've indexed, joined with the
    /// title. Powers the Map pane. Returned in arbitrary order; callers that
    /// care should sort.
    fn all_primary_geotags(&self) -> Result<Vec<MappedGeotag>>;

    // --- Category links ---

    /// Bulk insert categorylinks rows in one transaction. Returns rows
    /// processed.
    fn batch_upsert_categorylinks(&self, entries: &[crate::category::CategoryLink]) -> Result<u64>;
    /// Members of a category. `kind_filter` restricts to a single kind
    /// (`Page` for article members, `Subcat` for subcategories). Joins the
    /// articles table to resolve titles for page members; subcategory rows
    /// return the category name as title (best-effort, since the category
    /// page itself may not be in our articles table).
    fn category_members(
        &self,
        category: &str,
        kind_filter: Option<crate::category::CategoryMemberKind>,
        limit: u32,
    ) -> Result<Vec<crate::category::CategoryMember>>;
    /// Categories that contain an article (only `page` kind links).
    fn categories_for(&self, page_id: u64) -> Result<Vec<String>>;
    /// Distinct category names matching a prefix (case-insensitive). Used
    /// by the Browse pane's search input.
    fn search_categories(&self, prefix: &str, limit: u32) -> Result<Vec<String>>;
    fn count_categorylinks(&self) -> Result<u64>;

    /// Articles related to `page_id` by shared category membership. Returns
    /// up to `limit` rows ordered by descending shared-category count.
    /// Excludes the source article itself. Powers the Reader's "Related
    /// articles" section.
    fn related_to(&self, page_id: u64, limit: u32) -> Result<Vec<RelatedArticle>>;

    // --- Redirects ---

    fn batch_upsert_redirects(&self, entries: &[crate::redirect::Redirect]) -> Result<u64>;
    /// Resolve a redirect by source title. Returns the target title if the
    /// source title is a redirect we know about and the source's article
    /// record exists in storage. The Reader uses this so typing "USA" lands
    /// on "United States".
    fn resolve_redirect(&self, source_title: &Title) -> Result<Option<String>>;
    fn count_redirects(&self) -> Result<u64>;

    // --- Article embeddings (semantic search) ---

    /// Bulk insert/update embeddings in one transaction. Each entry is the
    /// page id, the raw f32 vector (variable dim, must match the model), and
    /// the model identifier (e.g. `"bge-small-en-v1.5"`). Existing rows for
    /// the same page_id are overwritten. Returns rows processed.
    fn batch_upsert_embeddings(&self, entries: &[(u64, Vec<f32>, &str)]) -> Result<u64>;
    /// Number of articles with an embedding stored under `model`. Used by
    /// the Settings UI to show ingest progress.
    fn count_embeddings(&self, model: &str) -> Result<u64>;
    /// Article ids that exist in `articles` but have no embedding for
    /// `model`. Used by the incremental embed-articles loop. Returned in
    /// page_id order; cap with `limit` so the caller can chunk the work.
    fn articles_without_embedding(&self, model: &str, limit: u32) -> Result<Vec<(u64, String)>>;
    /// Top-K articles by cosine similarity to `query`. Brute-force scan
    /// over every stored embedding for `model` — fast enough for
    /// simplewiki (~250K) but will need an HNSW index for full enwiki.
    /// Returns rows ordered by descending score in `[-1.0, 1.0]`.
    fn top_k_by_cosine(&self, model: &str, query: &[f32], k: u32) -> Result<Vec<EmbeddingHit>>;
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingHit {
    pub page_id: u64,
    pub title: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub page_id: u64,
    pub title: String,
    /// Unix epoch seconds of the last `touch()` call. 0 = never read.
    pub last_accessed: i64,
    /// Total reads since the last `clear_history()`.
    pub access_count: u32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MappedGeotag {
    pub page_id: u64,
    pub title: String,
    pub lat: f64,
    pub lon: f64,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RelatedArticle {
    pub page_id: u64,
    pub title: String,
    /// Number of categories this article shares with the source.
    pub shared_categories: u32,
}

pub struct SqliteArticleStore {
    conn: Mutex<Connection>,
}

impl SqliteArticleStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| TomeError::Storage(format!("open sqlite at {path:?}: {e}")))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| TomeError::Storage(format!("open in-memory sqlite: {e}")))?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| TomeError::Storage(format!("enable foreign keys: {e}")))?;
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| TomeError::Storage(format!("connection mutex poisoned: {e}")))
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_tier(s: &str) -> Result<Tier> {
    match s {
        "hot" => Ok(Tier::Hot),
        "warm" => Ok(Tier::Warm),
        "cold" => Ok(Tier::Cold),
        "evicted" => Ok(Tier::Evicted),
        other => Err(TomeError::Storage(format!("unknown tier: {other}"))),
    }
}

impl ArticleStore for SqliteArticleStore {
    fn upsert_metadata(&self, m: &ArticleMetadata) -> Result<()> {
        let conn = self.lock()?;
        let now_ts = now_secs();
        conn.execute(
            "INSERT INTO articles
                (page_id, title, tier, pinned, stream_offset, stream_length,
                 revision_id, last_accessed, access_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, ?8, ?8)
             ON CONFLICT(page_id) DO UPDATE SET
                title         = excluded.title,
                tier          = excluded.tier,
                pinned        = excluded.pinned,
                stream_offset = excluded.stream_offset,
                stream_length = excluded.stream_length,
                revision_id   = excluded.revision_id,
                updated_at    = excluded.updated_at",
            params![
                m.page_id as i64,
                m.title,
                m.tier.as_str(),
                m.pinned as i32,
                m.stream_offset.map(|v| v as i64),
                m.stream_length.map(|v| v as i64),
                m.revision_id.map(|v| v as i64),
                now_ts,
            ],
        )
        .map_err(|e| TomeError::Storage(format!("upsert metadata: {e}")))?;
        Ok(())
    }

    fn batch_upsert_cold(&self, entries: &[(u64, String, u64)]) -> Result<u64> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let now_ts = now_secs();
        let tx = conn
            .transaction()
            .map_err(|e| TomeError::Storage(format!("begin batch tx: {e}")))?;
        let mut count = 0_u64;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO articles
                        (page_id, title, tier, pinned, stream_offset, stream_length,
                         revision_id, last_accessed, access_count, created_at, updated_at)
                     VALUES (?1, ?2, 'cold', 0, ?3, NULL, NULL, 0, 0, ?4, ?4)
                     ON CONFLICT(page_id) DO UPDATE SET
                        title         = excluded.title,
                        stream_offset = excluded.stream_offset,
                        updated_at    = excluded.updated_at",
                )
                .map_err(|e| TomeError::Storage(format!("prepare batch: {e}")))?;
            for (page_id, title, stream_offset) in entries {
                stmt.execute(params![
                    *page_id as i64,
                    title,
                    *stream_offset as i64,
                    now_ts
                ])
                .map_err(|e| TomeError::Storage(format!("batch upsert: {e}")))?;
                count += 1;
            }
        }
        tx.commit()
            .map_err(|e| TomeError::Storage(format!("commit batch: {e}")))?;
        Ok(count)
    }

    fn lookup(&self, title: &Title) -> Result<Option<ArticleRecord>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT page_id, title, tier, pinned, stream_offset, stream_length,
                        revision_id, last_accessed, access_count
                 FROM articles WHERE title = ?1",
                params![title.as_str()],
                |row| {
                    let page_id: i64 = row.get(0)?;
                    let title: String = row.get(1)?;
                    let tier_str: String = row.get(2)?;
                    let pinned: i32 = row.get(3)?;
                    let stream_offset: Option<i64> = row.get(4)?;
                    let stream_length: Option<i64> = row.get(5)?;
                    let revision_id: Option<i64> = row.get(6)?;
                    let last_accessed: i64 = row.get(7)?;
                    let access_count: i64 = row.get(8)?;
                    Ok((
                        page_id,
                        title,
                        tier_str,
                        pinned,
                        stream_offset,
                        stream_length,
                        revision_id,
                        last_accessed,
                        access_count,
                    ))
                },
            )
            .optional()
            .map_err(|e| TomeError::Storage(format!("lookup: {e}")))?;

        match row {
            None => Ok(None),
            Some((page_id, title, tier_str, pinned, off, len, rev, last, count)) => {
                Ok(Some(ArticleRecord {
                    metadata: ArticleMetadata {
                        page_id: page_id as u64,
                        title,
                        tier: parse_tier(&tier_str)?,
                        pinned: pinned != 0,
                        stream_offset: off.map(|v| v as u64),
                        stream_length: len.map(|v| v as u64),
                        revision_id: rev.map(|v| v as u64),
                    },
                    last_accessed: last,
                    access_count: count as u64,
                }))
            }
        }
    }

    fn get_content(&self, page_id: u64) -> Result<Option<ArticleContent>> {
        let conn = self.lock()?;
        let row: Option<(String, Option<i64>, Option<i64>)> = conn
            .query_row(
                "SELECT tier, stream_offset, stream_length FROM articles WHERE page_id = ?1",
                params![page_id as i64],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| TomeError::Storage(format!("get_content metadata: {e}")))?;

        let Some((tier_str, off, len)) = row else {
            return Ok(None);
        };
        let tier = parse_tier(&tier_str)?;
        match tier {
            Tier::Hot => {
                let txt: String = conn
                    .query_row(
                        "SELECT wikitext FROM hot_content WHERE page_id = ?1",
                        params![page_id as i64],
                        |row| row.get(0),
                    )
                    .map_err(|e| TomeError::Storage(format!("read hot: {e}")))?;
                Ok(Some(ArticleContent::Hot(txt)))
            }
            Tier::Warm => {
                let blob: Vec<u8> = conn
                    .query_row(
                        "SELECT wikitext_zstd FROM warm_content WHERE page_id = ?1",
                        params![page_id as i64],
                        |row| row.get(0),
                    )
                    .map_err(|e| TomeError::Storage(format!("read warm: {e}")))?;
                let bytes = compression::decompress(&blob)?;
                let txt = String::from_utf8(bytes)
                    .map_err(|e| TomeError::Storage(format!("warm not utf-8: {e}")))?;
                Ok(Some(ArticleContent::Warm(txt)))
            }
            Tier::Cold => match off {
                Some(o) => Ok(Some(ArticleContent::Cold {
                    stream_offset: o as u64,
                    stream_length: len.map(|v| v as u64),
                })),
                None => Err(TomeError::Storage(format!(
                    "cold article {page_id} has no stream_offset"
                ))),
            },
            Tier::Evicted => Ok(Some(ArticleContent::Evicted)),
        }
    }

    fn set_tier(&self, page_id: u64, tier: Tier, content: Option<&str>) -> Result<()> {
        if matches!(tier, Tier::Hot | Tier::Warm) && content.is_none() {
            return Err(TomeError::Storage(format!(
                "tier {} requires content",
                tier.as_str()
            )));
        }
        if matches!(tier, Tier::Cold) {
            // Cold requires stream_offset to already be recorded.
            let conn = self.lock()?;
            let off: Option<i64> = conn
                .query_row(
                    "SELECT stream_offset FROM articles WHERE page_id = ?1",
                    params![page_id as i64],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| TomeError::Storage(format!("check cold offset: {e}")))?
                .flatten();
            if off.is_none() {
                return Err(TomeError::Storage(format!(
                    "page {page_id} cannot become Cold without stream_offset"
                )));
            }
        }

        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| TomeError::Storage(format!("begin tx: {e}")))?;

        // Update the metadata first so we surface NotFound before doing any
        // content work. INSERTs into the content tables would otherwise fail
        // on the foreign-key constraint and mask the real diagnosis.
        let updated = tx
            .execute(
                "UPDATE articles SET tier = ?1, updated_at = ?2 WHERE page_id = ?3",
                params![tier.as_str(), now_secs(), page_id as i64],
            )
            .map_err(|e| TomeError::Storage(format!("update tier: {e}")))?;
        if updated == 0 {
            return Err(TomeError::NotFound(format!("page_id {page_id}")));
        }

        // Drop any previously stored content; re-add below if needed.
        tx.execute(
            "DELETE FROM hot_content WHERE page_id = ?1",
            params![page_id as i64],
        )
        .map_err(|e| TomeError::Storage(format!("clear hot: {e}")))?;
        tx.execute(
            "DELETE FROM warm_content WHERE page_id = ?1",
            params![page_id as i64],
        )
        .map_err(|e| TomeError::Storage(format!("clear warm: {e}")))?;

        match (tier, content) {
            (Tier::Hot, Some(c)) => {
                tx.execute(
                    "INSERT INTO hot_content (page_id, wikitext) VALUES (?1, ?2)",
                    params![page_id as i64, c],
                )
                .map_err(|e| TomeError::Storage(format!("insert hot: {e}")))?;
            }
            (Tier::Warm, Some(c)) => {
                let compressed = compression::compress(c.as_bytes(), compression::DEFAULT_LEVEL)?;
                tx.execute(
                    "INSERT INTO warm_content (page_id, wikitext_zstd) VALUES (?1, ?2)",
                    params![page_id as i64, compressed],
                )
                .map_err(|e| TomeError::Storage(format!("insert warm: {e}")))?;
            }
            _ => {}
        }

        tx.commit()
            .map_err(|e| TomeError::Storage(format!("commit set_tier: {e}")))?;
        Ok(())
    }

    fn pin(&self, page_id: u64, pinned: bool) -> Result<()> {
        let conn = self.lock()?;
        let updated = conn
            .execute(
                "UPDATE articles SET pinned = ?1, updated_at = ?2 WHERE page_id = ?3",
                params![pinned as i32, now_secs(), page_id as i64],
            )
            .map_err(|e| TomeError::Storage(format!("pin: {e}")))?;
        if updated == 0 {
            return Err(TomeError::NotFound(format!("page_id {page_id}")));
        }
        Ok(())
    }

    fn touch(&self, page_id: u64) -> Result<()> {
        let conn = self.lock()?;
        let updated = conn
            .execute(
                "UPDATE articles
                    SET last_accessed = ?1, access_count = access_count + 1
                  WHERE page_id = ?2",
                params![now_secs(), page_id as i64],
            )
            .map_err(|e| TomeError::Storage(format!("touch: {e}")))?;
        if updated == 0 {
            return Err(TomeError::NotFound(format!("page_id {page_id}")));
        }
        Ok(())
    }

    fn count_by_tier(&self, tier: Tier) -> Result<u64> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM articles WHERE tier = ?1",
                params![tier.as_str()],
                |row| row.get(0),
            )
            .map_err(|e| TomeError::Storage(format!("count tier: {e}")))?;
        Ok(n as u64)
    }

    fn batch_upsert_geotags(&self, entries: &[crate::geotag::Geotag]) -> Result<u64> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| TomeError::Storage(format!("begin geotag tx: {e}")))?;
        let mut count = 0_u64;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO geotags (page_id, lat, lon, primary_, kind)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(page_id, lat, lon) DO UPDATE SET
                        primary_ = excluded.primary_,
                        kind     = excluded.kind",
                )
                .map_err(|e| TomeError::Storage(format!("prepare geotag: {e}")))?;
            for g in entries {
                stmt.execute(params![
                    g.page_id as i64,
                    g.lat,
                    g.lon,
                    g.primary as i32,
                    g.kind.as_deref()
                ])
                .map_err(|e| TomeError::Storage(format!("upsert geotag: {e}")))?;
                count += 1;
            }
        }
        tx.commit()
            .map_err(|e| TomeError::Storage(format!("commit geotag: {e}")))?;
        Ok(count)
    }

    fn geotag_for(&self, page_id: u64) -> Result<Option<crate::geotag::Geotag>> {
        let conn = self.lock()?;
        // Prefer primary; fall back to any.
        conn.query_row(
            "SELECT page_id, lat, lon, primary_, kind FROM geotags
             WHERE page_id = ?1
             ORDER BY primary_ DESC LIMIT 1",
            params![page_id as i64],
            |row| {
                Ok(crate::geotag::Geotag {
                    page_id: row.get::<_, i64>(0)? as u64,
                    lat: row.get(1)?,
                    lon: row.get(2)?,
                    primary: row.get::<_, i32>(3)? != 0,
                    kind: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|e| TomeError::Storage(format!("geotag_for: {e}")))
    }

    fn count_geotags(&self) -> Result<u64> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM geotags", [], |row| row.get(0))
            .map_err(|e| TomeError::Storage(format!("count geotags: {e}")))?;
        Ok(n as u64)
    }

    fn all_primary_geotags(&self) -> Result<Vec<MappedGeotag>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT g.page_id, a.title, g.lat, g.lon, g.kind
                 FROM geotags g
                 JOIN articles a ON a.page_id = g.page_id
                 WHERE g.primary_ = 1",
            )
            .map_err(|e| TomeError::Storage(format!("prepare all_primary_geotags: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MappedGeotag {
                    page_id: row.get::<_, i64>(0)? as u64,
                    title: row.get(1)?,
                    lat: row.get(2)?,
                    lon: row.get(3)?,
                    kind: row.get(4)?,
                })
            })
            .map_err(|e| TomeError::Storage(format!("query all_primary_geotags: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| TomeError::Storage(format!("row all_primary_geotags: {e}")))?);
        }
        Ok(out)
    }

    fn batch_upsert_categorylinks(&self, entries: &[crate::category::CategoryLink]) -> Result<u64> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| TomeError::Storage(format!("begin categorylinks tx: {e}")))?;
        let mut count = 0_u64;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO categorylinks (cl_from, cl_to, cl_type)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(cl_from, cl_to) DO UPDATE SET cl_type = excluded.cl_type",
                )
                .map_err(|e| TomeError::Storage(format!("prepare categorylinks: {e}")))?;
            for link in entries {
                stmt.execute(params![
                    link.from_page_id as i64,
                    link.category,
                    link.kind.as_str(),
                ])
                .map_err(|e| TomeError::Storage(format!("upsert categorylink: {e}")))?;
                count += 1;
            }
        }
        tx.commit()
            .map_err(|e| TomeError::Storage(format!("commit categorylinks: {e}")))?;
        Ok(count)
    }

    fn category_members(
        &self,
        category: &str,
        kind_filter: Option<crate::category::CategoryMemberKind>,
        limit: u32,
    ) -> Result<Vec<crate::category::CategoryMember>> {
        let conn = self.lock()?;
        let mut sql = String::from(
            "SELECT cl.cl_from, cl.cl_type, COALESCE(a.title, REPLACE(cl.cl_to, '_', ' ')) AS display_title \
             FROM categorylinks cl LEFT JOIN articles a ON a.page_id = cl.cl_from \
             WHERE cl.cl_to = ?1",
        );
        // Bind positions are positional; the LIMIT placeholder index has to
        // match the actual count of bound params. Without a kind filter we
        // bind 2 (key + limit); with one we bind 3.
        if kind_filter.is_some() {
            sql.push_str(" AND cl.cl_type = ?2 ORDER BY display_title LIMIT ?3");
        } else {
            sql.push_str(" ORDER BY display_title LIMIT ?2");
        }

        // Categories on Wikipedia use underscores in their internal form; the
        // user-facing input may use spaces. Normalize the lookup key to the
        // underscore form so either matches.
        let key = category.replace(' ', "_");

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| TomeError::Storage(format!("prepare category_members: {e}")))?;

        let map_row =
            |row: &rusqlite::Row<'_>| -> rusqlite::Result<crate::category::CategoryMember> {
                let from: i64 = row.get(0)?;
                let kind_str: String = row.get(1)?;
                let title: String = row.get(2)?;
                let kind = crate::category::CategoryMemberKind::parse(&kind_str)
                    .unwrap_or(crate::category::CategoryMemberKind::Page);
                Ok(crate::category::CategoryMember {
                    kind,
                    title,
                    page_id: from as u64,
                })
            };

        let rows: Vec<_> = if let Some(k) = kind_filter {
            stmt.query_map(params![key, k.as_str(), limit as i64], map_row)
                .map_err(|e| TomeError::Storage(format!("query category_members: {e}")))?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| TomeError::Storage(format!("collect category_members: {e}")))?
        } else {
            stmt.query_map(params![key, limit as i64], map_row)
                .map_err(|e| TomeError::Storage(format!("query category_members: {e}")))?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| TomeError::Storage(format!("collect category_members: {e}")))?
        };

        Ok(rows)
    }

    fn categories_for(&self, page_id: u64) -> Result<Vec<String>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT cl_to FROM categorylinks
                 WHERE cl_from = ?1 AND cl_type = 'page'
                 ORDER BY cl_to",
            )
            .map_err(|e| TomeError::Storage(format!("prepare categories_for: {e}")))?;
        let rows = stmt
            .query_map(params![page_id as i64], |row| {
                row.get::<_, String>(0).map(|s| s.replace('_', " "))
            })
            .map_err(|e| TomeError::Storage(format!("query categories_for: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| TomeError::Storage(format!("row: {e}")))?);
        }
        Ok(out)
    }

    fn search_categories(&self, prefix: &str, limit: u32) -> Result<Vec<String>> {
        let conn = self.lock()?;
        // SQLite's LIKE is case-insensitive for ASCII by default. Normalize
        // spaces to underscores so either form matches, then escape the
        // wildcard meta-characters (% _ \) so a user typing "100%" doesn't
        // match every category and "_help" doesn't gobble preceding chars.
        let normalized = prefix.replace(' ', "_");
        let mut escaped = String::with_capacity(normalized.len() + 4);
        for c in normalized.chars() {
            match c {
                '\\' | '%' | '_' => {
                    escaped.push('\\');
                    escaped.push(c);
                }
                _ => escaped.push(c),
            }
        }
        escaped.push('%');
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT cl_to FROM categorylinks
                 WHERE cl_to LIKE ?1 ESCAPE '\\'
                 ORDER BY cl_to LIMIT ?2",
            )
            .map_err(|e| TomeError::Storage(format!("prepare search_categories: {e}")))?;
        let rows = stmt
            .query_map(params![escaped, limit as i64], |row| {
                row.get::<_, String>(0).map(|s| s.replace('_', " "))
            })
            .map_err(|e| TomeError::Storage(format!("query search_categories: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| TomeError::Storage(format!("row: {e}")))?);
        }
        Ok(out)
    }

    fn count_categorylinks(&self) -> Result<u64> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM categorylinks", [], |row| row.get(0))
            .map_err(|e| TomeError::Storage(format!("count categorylinks: {e}")))?;
        Ok(n as u64)
    }

    fn batch_upsert_redirects(&self, entries: &[crate::redirect::Redirect]) -> Result<u64> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| TomeError::Storage(format!("begin redirects tx: {e}")))?;
        let mut count = 0_u64;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO redirects (from_page_id, target_title)
                     VALUES (?1, ?2)
                     ON CONFLICT(from_page_id) DO UPDATE SET
                        target_title = excluded.target_title",
                )
                .map_err(|e| TomeError::Storage(format!("prepare redirect upsert: {e}")))?;
            for r in entries {
                stmt.execute(params![r.from_page_id as i64, r.target_title])
                    .map_err(|e| TomeError::Storage(format!("upsert redirect: {e}")))?;
                count += 1;
            }
        }
        tx.commit()
            .map_err(|e| TomeError::Storage(format!("commit redirects: {e}")))?;
        Ok(count)
    }

    fn resolve_redirect(&self, source_title: &Title) -> Result<Option<String>> {
        let conn = self.lock()?;
        // First find the source title's page_id, then look up its redirect target.
        let page_id: Option<i64> = conn
            .query_row(
                "SELECT page_id FROM articles WHERE title = ?1",
                params![source_title.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| TomeError::Storage(format!("redirect source lookup: {e}")))?;
        let Some(pid) = page_id else {
            return Ok(None);
        };
        conn.query_row(
            "SELECT target_title FROM redirects WHERE from_page_id = ?1",
            params![pid],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| TomeError::Storage(format!("redirect resolve: {e}")))
    }

    fn count_redirects(&self) -> Result<u64> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM redirects", [], |row| row.get(0))
            .map_err(|e| TomeError::Storage(format!("count redirects: {e}")))?;
        Ok(n as u64)
    }

    fn related_to(&self, page_id: u64, limit: u32) -> Result<Vec<RelatedArticle>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "WITH src_cats AS (
                     SELECT cl_to FROM categorylinks
                     WHERE cl_from = ?1 AND cl_type = 'page'
                 )
                 SELECT a.page_id, a.title, COUNT(*) AS shared
                 FROM categorylinks cl
                 JOIN src_cats sc ON cl.cl_to = sc.cl_to
                 JOIN articles a ON a.page_id = cl.cl_from
                 WHERE cl.cl_from != ?1 AND cl.cl_type = 'page'
                 GROUP BY a.page_id, a.title
                 ORDER BY shared DESC, a.title
                 LIMIT ?2",
            )
            .map_err(|e| TomeError::Storage(format!("prepare related_to: {e}")))?;
        let rows = stmt
            .query_map(params![page_id as i64, limit as i64], |row| {
                let pid: i64 = row.get(0)?;
                let title: String = row.get(1)?;
                let shared: i64 = row.get(2)?;
                Ok(RelatedArticle {
                    page_id: pid as u64,
                    title,
                    shared_categories: shared as u32,
                })
            })
            .map_err(|e| TomeError::Storage(format!("query related_to: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| TomeError::Storage(format!("row: {e}")))?);
        }
        Ok(out)
    }

    fn lru_candidates(&self, n: u32) -> Result<Vec<u64>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT page_id FROM articles
                  WHERE tier IN ('hot','warm') AND pinned = 0
                  ORDER BY last_accessed ASC, access_count ASC, page_id ASC
                  LIMIT ?1",
            )
            .map_err(|e| TomeError::Storage(format!("prepare lru: {e}")))?;
        let rows = stmt
            .query_map(params![n as i64], |row| row.get::<_, i64>(0))
            .map_err(|e| TomeError::Storage(format!("query lru: {e}")))?;
        let ids: rusqlite::Result<Vec<i64>> = rows.collect();
        Ok(ids
            .map_err(|e| TomeError::Storage(format!("collect lru: {e}")))?
            .into_iter()
            .map(|v| v as u64)
            .collect())
    }

    fn random_article_title(&self) -> Result<Option<String>> {
        let conn = self.lock()?;
        // ORDER BY RANDOM() does a full table scan but the LIMIT 1 keeps
        // memory bounded and the corpus tops out at ~6.8 M rows for
        // enwiki — well under a second on any modern disk. For an
        // interactive button click that's plenty.
        // Excluding 'evicted' so a user who deliberately marked an
        // article as off-limits doesn't get bounced into it.
        conn.query_row(
            "SELECT title FROM articles
             WHERE tier IN ('hot','warm','cold')
             ORDER BY RANDOM() LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| TomeError::Storage(format!("random_article_title: {e}")))
    }

    fn recent_articles(&self, limit: u32) -> Result<Vec<HistoryEntry>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT page_id, title, last_accessed, access_count
                 FROM articles
                 WHERE last_accessed > 0
                 ORDER BY last_accessed DESC
                 LIMIT ?1",
            )
            .map_err(|e| TomeError::Storage(format!("prepare recent_articles: {e}")))?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(HistoryEntry {
                    page_id: row.get::<_, i64>(0)? as u64,
                    title: row.get(1)?,
                    last_accessed: row.get(2)?,
                    access_count: row.get::<_, i64>(3)? as u32,
                })
            })
            .map_err(|e| TomeError::Storage(format!("query recent_articles: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| TomeError::Storage(format!("row recent: {e}")))?);
        }
        Ok(out)
    }

    fn clear_history(&self) -> Result<u64> {
        let conn = self.lock()?;
        // Touch only rows that were actually read so the row-count we
        // return is meaningful.
        let n = conn
            .execute(
                "UPDATE articles
                 SET last_accessed = 0, access_count = 0
                 WHERE last_accessed > 0 OR access_count > 0",
                [],
            )
            .map_err(|e| TomeError::Storage(format!("clear_history: {e}")))?;
        Ok(n as u64)
    }

    fn add_bookmark(
        &self,
        article_title: &str,
        folder_id: Option<i64>,
        note: Option<&str>,
    ) -> Result<i64> {
        let conn = self.lock()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // Idempotent: if a bookmark already exists for the same
        // (article_title, folder_id) UNIQUE pair, return its id rather
        // than erroring out. Folder_id NULL participates in the unique
        // index per SQLite semantics for nullable composite uniques.
        // To handle that we explicitly look for an existing match
        // first.
        let existing: Option<i64> = match folder_id {
            Some(fid) => conn
                .query_row(
                    "SELECT id FROM bookmarks WHERE article_title = ?1 AND folder_id = ?2",
                    params![article_title, fid],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| TomeError::Storage(format!("check existing bookmark: {e}")))?,
            None => conn
                .query_row(
                    "SELECT id FROM bookmarks WHERE article_title = ?1 AND folder_id IS NULL",
                    params![article_title],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| TomeError::Storage(format!("check existing bookmark: {e}")))?,
        };
        if let Some(id) = existing {
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO bookmarks (article_title, folder_id, note, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![article_title, folder_id, note, now],
        )
        .map_err(|e| TomeError::Storage(format!("insert bookmark: {e}")))?;
        Ok(conn.last_insert_rowid())
    }

    fn remove_bookmark(&self, id: i64) -> Result<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])
            .map_err(|e| TomeError::Storage(format!("delete bookmark: {e}")))?;
        Ok(())
    }

    fn move_bookmark(&self, id: i64, folder_id: Option<i64>) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE bookmarks SET folder_id = ?1 WHERE id = ?2",
            params![folder_id, id],
        )
        .map_err(|e| TomeError::Storage(format!("move bookmark: {e}")))?;
        Ok(())
    }

    fn is_bookmarked(&self, article_title: &str) -> Result<bool> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM bookmarks WHERE article_title = ?1",
                params![article_title],
                |row| row.get(0),
            )
            .map_err(|e| TomeError::Storage(format!("is_bookmarked: {e}")))?;
        Ok(n > 0)
    }

    fn bookmarks_in_folder(
        &self,
        folder_id: Option<i64>,
        limit: u32,
    ) -> Result<Vec<crate::bookmark::Bookmark>> {
        let conn = self.lock()?;
        let limit = limit.clamp(1, 10_000) as i64;
        // Two prepared statements because SQLite's `?` placeholder
        // doesn't substitute into `IS NULL` cleanly; querying
        // `folder_id = NULL` always returns no rows.
        let out: Vec<crate::bookmark::Bookmark> = match folder_id {
            Some(fid) => {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, article_title, folder_id, note, created_at
                         FROM bookmarks WHERE folder_id = ?1
                         ORDER BY created_at DESC LIMIT ?2",
                    )
                    .map_err(|e| TomeError::Storage(format!("prepare bookmarks: {e}")))?;
                stmt.query_map(params![fid, limit], bookmark_row_to_struct)
                    .map_err(|e| TomeError::Storage(format!("query bookmarks: {e}")))?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| TomeError::Storage(format!("collect bookmarks: {e}")))?
            }
            None => {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, article_title, folder_id, note, created_at
                         FROM bookmarks WHERE folder_id IS NULL
                         ORDER BY created_at DESC LIMIT ?1",
                    )
                    .map_err(|e| TomeError::Storage(format!("prepare bookmarks: {e}")))?;
                stmt.query_map(params![limit], bookmark_row_to_struct)
                    .map_err(|e| TomeError::Storage(format!("query bookmarks: {e}")))?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| TomeError::Storage(format!("collect bookmarks: {e}")))?
            }
        };
        Ok(out)
    }

    fn all_bookmarks(&self, limit: u32) -> Result<Vec<crate::bookmark::Bookmark>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, article_title, folder_id, note, created_at
                 FROM bookmarks ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| TomeError::Storage(format!("prepare all_bookmarks: {e}")))?;
        let rows: rusqlite::Result<Vec<_>> = stmt
            .query_map(
                params![limit.clamp(1, 10_000) as i64],
                bookmark_row_to_struct,
            )
            .map_err(|e| TomeError::Storage(format!("query all_bookmarks: {e}")))?
            .collect();
        rows.map_err(|e| TomeError::Storage(format!("collect all_bookmarks: {e}")))
    }

    fn count_bookmarks(&self) -> Result<u64> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM bookmarks", [], |row| row.get(0))
            .map_err(|e| TomeError::Storage(format!("count bookmarks: {e}")))?;
        Ok(n as u64)
    }

    fn create_folder(&self, name: &str, parent_id: Option<i64>) -> Result<i64> {
        let conn = self.lock()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO bookmark_folders (name, parent_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![name, parent_id, now],
        )
        .map_err(|e| TomeError::Storage(format!("create folder: {e}")))?;
        Ok(conn.last_insert_rowid())
    }

    fn rename_folder(&self, id: i64, new_name: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE bookmark_folders SET name = ?1 WHERE id = ?2",
            params![new_name, id],
        )
        .map_err(|e| TomeError::Storage(format!("rename folder: {e}")))?;
        Ok(())
    }

    fn delete_folder(&self, id: i64) -> Result<()> {
        let conn = self.lock()?;
        // ON DELETE SET NULL on bookmarks.folder_id un-files bookmarks
        // automatically; ON DELETE CASCADE on bookmark_folders.parent_id
        // recursively removes subfolders.
        conn.execute("DELETE FROM bookmark_folders WHERE id = ?1", params![id])
            .map_err(|e| TomeError::Storage(format!("delete folder: {e}")))?;
        Ok(())
    }

    fn list_folders(&self) -> Result<Vec<crate::bookmark::BookmarkFolder>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, parent_id, created_at
                 FROM bookmark_folders ORDER BY name COLLATE NOCASE",
            )
            .map_err(|e| TomeError::Storage(format!("prepare list_folders: {e}")))?;
        let rows: rusqlite::Result<Vec<_>> = stmt
            .query_map([], |row| {
                Ok(crate::bookmark::BookmarkFolder {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    parent_id: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| TomeError::Storage(format!("query list_folders: {e}")))?
            .collect();
        rows.map_err(|e| TomeError::Storage(format!("collect list_folders: {e}")))
    }

    fn batch_upsert_embeddings(&self, entries: &[(u64, Vec<f32>, &str)]) -> Result<u64> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock()?;
        let tx = conn
            .transaction()
            .map_err(|e| TomeError::Storage(format!("begin embeddings tx: {e}")))?;
        let mut count = 0_u64;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO article_embeddings (page_id, embedding, model)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(page_id) DO UPDATE
                       SET embedding = excluded.embedding,
                           model = excluded.model",
                )
                .map_err(|e| TomeError::Storage(format!("prepare embedding upsert: {e}")))?;
            for (page_id, vec, model) in entries {
                let bytes = vec_f32_to_blob(vec);
                stmt.execute(params![*page_id as i64, bytes, *model])
                    .map_err(|e| TomeError::Storage(format!("upsert embedding: {e}")))?;
                count += 1;
            }
        }
        tx.commit()
            .map_err(|e| TomeError::Storage(format!("commit embeddings: {e}")))?;
        Ok(count)
    }

    fn count_embeddings(&self, model: &str) -> Result<u64> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM article_embeddings WHERE model = ?1",
                params![model],
                |row| row.get(0),
            )
            .map_err(|e| TomeError::Storage(format!("count embeddings: {e}")))?;
        Ok(n as u64)
    }

    fn articles_without_embedding(&self, model: &str, limit: u32) -> Result<Vec<(u64, String)>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT a.page_id, a.title
                 FROM articles a
                 LEFT JOIN article_embeddings e
                   ON e.page_id = a.page_id AND e.model = ?1
                 WHERE e.page_id IS NULL
                 ORDER BY a.page_id
                 LIMIT ?2",
            )
            .map_err(|e| TomeError::Storage(format!("prepare missing-emb: {e}")))?;
        let rows = stmt
            .query_map(params![model, limit as i64], |row| {
                let id: i64 = row.get(0)?;
                let title: String = row.get(1)?;
                Ok((id as u64, title))
            })
            .map_err(|e| TomeError::Storage(format!("query missing-emb: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| TomeError::Storage(format!("row missing-emb: {e}")))?);
        }
        Ok(out)
    }

    fn top_k_by_cosine(&self, model: &str, query: &[f32], k: u32) -> Result<Vec<EmbeddingHit>> {
        if query.is_empty() || k == 0 {
            return Ok(Vec::new());
        }
        // Pre-compute the query's L2 norm once. Per-row work then becomes
        // dot(query, row) / (|query| * |row|).
        let q_norm_sq: f32 = query.iter().map(|x| x * x).sum();
        let q_norm = q_norm_sq.sqrt();
        if q_norm == 0.0 {
            return Ok(Vec::new());
        }

        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT e.page_id, a.title, e.embedding
                 FROM article_embeddings e
                 JOIN articles a ON a.page_id = e.page_id
                 WHERE e.model = ?1",
            )
            .map_err(|e| TomeError::Storage(format!("prepare cosine scan: {e}")))?;
        let rows = stmt
            .query_map(params![model], |row| {
                let id: i64 = row.get(0)?;
                let title: String = row.get(1)?;
                let blob: Vec<u8> = row.get(2)?;
                Ok((id as u64, title, blob))
            })
            .map_err(|e| TomeError::Storage(format!("query cosine scan: {e}")))?;

        // Maintain a fixed-size top-K min-heap so we never hold more than K
        // candidates in memory regardless of corpus size.
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;

        #[derive(PartialEq)]
        struct Candidate {
            score: f32,
            page_id: u64,
            title: String,
        }
        impl Eq for Candidate {}
        impl Ord for Candidate {
            fn cmp(&self, other: &Self) -> Ordering {
                // Reverse so BinaryHeap's max-heap behaves as min-heap on score.
                other
                    .score
                    .partial_cmp(&self.score)
                    .unwrap_or(Ordering::Equal)
            }
        }
        impl PartialOrd for Candidate {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        // The heap holds at most `cap` candidates after every push, but
        // we don't preallocate the full capacity up front — a malicious or
        // pathological k (u32::MAX) would otherwise allocate ~170 GB and
        // OOM the process. Start small and let it grow naturally.
        let cap = k as usize;
        let mut heap: BinaryHeap<Candidate> =
            BinaryHeap::with_capacity(cap.min(1024).saturating_add(1));
        for r in rows {
            let (page_id, title, blob) =
                r.map_err(|e| TomeError::Storage(format!("row cosine: {e}")))?;
            let row_vec = match blob_to_vec_f32(&blob) {
                Some(v) if v.len() == query.len() => v,
                _ => continue, // dim mismatch or corrupt blob — skip rather than fail the whole search
            };
            let mut dot = 0.0f32;
            let mut row_norm_sq = 0.0f32;
            for (a, b) in query.iter().zip(row_vec.iter()) {
                dot += a * b;
                row_norm_sq += b * b;
            }
            if row_norm_sq == 0.0 {
                continue;
            }
            let score = dot / (q_norm * row_norm_sq.sqrt());
            heap.push(Candidate {
                score,
                page_id,
                title,
            });
            if heap.len() > cap {
                heap.pop();
            }
        }

        let mut out: Vec<EmbeddingHit> = heap
            .into_iter()
            .map(|c| EmbeddingHit {
                page_id: c.page_id,
                title: c.title,
                score: c.score,
            })
            .collect();
        out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        Ok(out)
    }
}

/// Encode a vector of f32 as little-endian bytes for SQLite BLOB storage.
/// Map a SELECT id, article_title, folder_id, note, created_at row to a
/// Bookmark struct. Pulled out into a free function so the multiple
/// query sites in the impl can share it.
fn bookmark_row_to_struct(row: &rusqlite::Row<'_>) -> rusqlite::Result<crate::bookmark::Bookmark> {
    Ok(crate::bookmark::Bookmark {
        id: row.get(0)?,
        article_title: row.get(1)?,
        folder_id: row.get(2)?,
        note: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn vec_f32_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// Decode a SQLite BLOB back into f32 values. Returns `None` if the blob
/// length isn't a multiple of 4 (i.e. it's corrupt). Does not validate
/// dimension; the caller should compare to the expected `dim()`.
fn blob_to_vec_f32(b: &[u8]) -> Option<Vec<f32>> {
    if b.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(b.len() / 4);
    for chunk in b.chunks_exact(4) {
        let arr: [u8; 4] = chunk.try_into().ok()?;
        out.push(f32::from_le_bytes(arr));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(page_id: u64, title: &str, tier: Tier) -> ArticleMetadata {
        ArticleMetadata {
            page_id,
            title: title.into(),
            tier,
            pinned: false,
            stream_offset: None,
            stream_length: None,
            revision_id: None,
        }
    }

    fn cold_meta(page_id: u64, title: &str, offset: u64) -> ArticleMetadata {
        ArticleMetadata {
            page_id,
            title: title.into(),
            tier: Tier::Cold,
            pinned: false,
            stream_offset: Some(offset),
            stream_length: Some(1024),
            revision_id: Some(1),
        }
    }

    #[test]
    fn upsert_then_lookup() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store
            .upsert_metadata(&cold_meta(42, "Photon", 1_000))
            .unwrap();
        let rec = store
            .lookup(&Title::new("Photon"))
            .unwrap()
            .expect("photon present");
        assert_eq!(rec.metadata.page_id, 42);
        assert_eq!(rec.metadata.tier, Tier::Cold);
        assert_eq!(rec.metadata.stream_offset, Some(1_000));
        assert_eq!(rec.metadata.stream_length, Some(1024));
        assert_eq!(rec.metadata.revision_id, Some(1));
        assert_eq!(rec.access_count, 0);
    }

    #[test]
    fn lookup_normalizes_title_underscores() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store
            .upsert_metadata(&cold_meta(1, "Higgs boson", 1))
            .unwrap();
        // Title::new converts underscores to spaces; this is the tome-core
        // normalization contract.
        let rec = store
            .lookup(&Title::new("Higgs_boson"))
            .unwrap()
            .expect("found via underscore form");
        assert_eq!(rec.metadata.page_id, 1);
    }

    #[test]
    fn lookup_missing_returns_none() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        assert!(store.lookup(&Title::new("Nonexistent")).unwrap().is_none());
    }

    #[test]
    fn promote_cold_to_hot_then_demote_to_warm_then_cold() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store.upsert_metadata(&cold_meta(1, "Photon", 100)).unwrap();

        // Cold -> Hot: content delivered from caller.
        store.set_tier(1, Tier::Hot, Some("photon body")).unwrap();
        match store.get_content(1).unwrap().unwrap() {
            ArticleContent::Hot(txt) => assert_eq!(txt, "photon body"),
            other => panic!("expected Hot, got {other:?}"),
        }

        // Hot -> Warm: content compressed and decompressed transparently.
        store.set_tier(1, Tier::Warm, Some("photon body")).unwrap();
        match store.get_content(1).unwrap().unwrap() {
            ArticleContent::Warm(txt) => assert_eq!(txt, "photon body"),
            other => panic!("expected Warm, got {other:?}"),
        }

        // Warm -> Cold: content dropped, metadata's stream_offset still works.
        store.set_tier(1, Tier::Cold, None).unwrap();
        match store.get_content(1).unwrap().unwrap() {
            ArticleContent::Cold {
                stream_offset,
                stream_length,
            } => {
                assert_eq!(stream_offset, 100);
                assert_eq!(stream_length, Some(1024));
            }
            other => panic!("expected Cold, got {other:?}"),
        }
    }

    #[test]
    fn tier_hot_without_content_errors() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store.upsert_metadata(&cold_meta(1, "X", 1)).unwrap();
        let err = store.set_tier(1, Tier::Hot, None).unwrap_err();
        assert!(matches!(err, TomeError::Storage(_)));
    }

    #[test]
    fn cold_without_offset_errors() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        // Insert a Hot article with no stream_offset, then try to demote to Cold.
        store.upsert_metadata(&meta(1, "X", Tier::Hot)).unwrap();
        store.set_tier(1, Tier::Hot, Some("body")).unwrap();
        let err = store.set_tier(1, Tier::Cold, None).unwrap_err();
        assert!(matches!(err, TomeError::Storage(_)));
    }

    #[test]
    fn evict_drops_content() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store.upsert_metadata(&cold_meta(1, "Photon", 100)).unwrap();
        store.set_tier(1, Tier::Hot, Some("body")).unwrap();
        store.set_tier(1, Tier::Evicted, None).unwrap();
        match store.get_content(1).unwrap().unwrap() {
            ArticleContent::Evicted => {}
            other => panic!("expected Evicted, got {other:?}"),
        }
    }

    #[test]
    fn set_tier_on_missing_article_errors() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let err = store.set_tier(999, Tier::Hot, Some("x")).unwrap_err();
        assert!(matches!(err, TomeError::NotFound(_)));
    }

    #[test]
    fn touch_increments_count_and_updates_timestamp() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store.upsert_metadata(&cold_meta(1, "X", 1)).unwrap();
        store.touch(1).unwrap();
        store.touch(1).unwrap();
        store.touch(1).unwrap();
        let rec = store.lookup(&Title::new("X")).unwrap().unwrap();
        assert_eq!(rec.access_count, 3);
        assert!(rec.last_accessed > 0);
    }

    #[test]
    fn pin_blocks_lru_candidates() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        for i in 1..=3 {
            store
                .upsert_metadata(&cold_meta(i, &format!("Page{i}"), i * 100))
                .unwrap();
            store
                .set_tier(i, Tier::Hot, Some(&format!("body {i}")))
                .unwrap();
            store.touch(i).unwrap();
        }
        store.pin(2, true).unwrap();
        let candidates = store.lru_candidates(10).unwrap();
        assert!(!candidates.contains(&2));
        assert!(candidates.contains(&1));
        assert!(candidates.contains(&3));
    }

    #[test]
    fn lru_candidates_orders_by_oldest_access_first() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        for i in 1..=3 {
            store
                .upsert_metadata(&cold_meta(i, &format!("Page{i}"), i * 100))
                .unwrap();
            store.set_tier(i, Tier::Hot, Some("body")).unwrap();
        }
        // Stagger accesses: 1 oldest, 3 newest. We can't sleep in unit tests
        // realistically, so we update last_accessed directly via touch order
        // and rely on monotonic-ish wall-clock. Use access_count tiebreaker:
        store.touch(2).unwrap(); // access_count=1
        store.touch(2).unwrap(); // access_count=2
        store.touch(3).unwrap(); // access_count=1
        // Page 1 has access_count=0; should appear first.
        let candidates = store.lru_candidates(3).unwrap();
        assert_eq!(candidates[0], 1, "least-accessed should be first");
    }

    #[test]
    fn batch_upsert_cold_inserts_many_rows_in_one_tx() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let entries: Vec<(u64, String, u64)> = (1..=1000)
            .map(|i| (i, format!("Article{i}"), i * 100))
            .collect();
        let n = store.batch_upsert_cold(&entries).unwrap();
        assert_eq!(n, 1000);
        assert_eq!(store.count_by_tier(Tier::Cold).unwrap(), 1000);
        let rec = store
            .lookup(&Title::new("Article500"))
            .unwrap()
            .expect("found");
        assert_eq!(rec.metadata.page_id, 500);
        assert_eq!(rec.metadata.stream_offset, Some(50_000));
        assert_eq!(rec.metadata.tier, Tier::Cold);
    }

    #[test]
    fn batch_upsert_cold_updates_existing_rows() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store
            .batch_upsert_cold(&[(1, "Photon".into(), 100)])
            .unwrap();
        // Re-ingest with a different offset (e.g. new dump).
        store
            .batch_upsert_cold(&[(1, "Photon".into(), 999)])
            .unwrap();
        let rec = store.lookup(&Title::new("Photon")).unwrap().unwrap();
        assert_eq!(rec.metadata.stream_offset, Some(999));
        assert_eq!(store.count_by_tier(Tier::Cold).unwrap(), 1);
    }

    #[test]
    fn batch_upsert_cold_empty_input_no_op() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let n = store.batch_upsert_cold(&[]).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn count_by_tier_distinguishes_tiers() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store.upsert_metadata(&cold_meta(1, "Cold1", 1)).unwrap();
        store.upsert_metadata(&cold_meta(2, "Cold2", 2)).unwrap();
        store.upsert_metadata(&cold_meta(3, "Hot1", 3)).unwrap();
        store.set_tier(3, Tier::Hot, Some("hot body")).unwrap();
        assert_eq!(store.count_by_tier(Tier::Cold).unwrap(), 2);
        assert_eq!(store.count_by_tier(Tier::Hot).unwrap(), 1);
        assert_eq!(store.count_by_tier(Tier::Warm).unwrap(), 0);
        assert_eq!(store.count_by_tier(Tier::Evicted).unwrap(), 0);
    }

    // --- Embedding tests --------------------------------------------------

    fn seed_articles(store: &SqliteArticleStore, n: u64) {
        for i in 1..=n {
            store
                .upsert_metadata(&cold_meta(i, &format!("A{i}"), i * 100))
                .unwrap();
        }
    }

    #[test]
    fn embeddings_round_trip_through_blob_storage() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        seed_articles(&store, 3);

        // Use distinct vectors so cosine ranks them deterministically.
        let entries = vec![
            (1u64, vec![1.0f32, 0.0, 0.0], "test-model"),
            (2u64, vec![0.0f32, 1.0, 0.0], "test-model"),
            (3u64, vec![0.5f32, 0.5, 0.0], "test-model"),
        ];
        let n = store.batch_upsert_embeddings(&entries).unwrap();
        assert_eq!(n, 3);
        assert_eq!(store.count_embeddings("test-model").unwrap(), 3);
        assert_eq!(store.count_embeddings("other-model").unwrap(), 0);
    }

    #[test]
    fn upsert_embedding_overwrites_existing_row() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        seed_articles(&store, 1);
        store
            .batch_upsert_embeddings(&[(1, vec![1.0, 0.0], "m")])
            .unwrap();
        store
            .batch_upsert_embeddings(&[(1, vec![0.0, 1.0], "m")])
            .unwrap();
        // Still only one row for this page+model.
        assert_eq!(store.count_embeddings("m").unwrap(), 1);
        // And cosine should now match the new vector, not the old.
        let hits = store.top_k_by_cosine("m", &[0.0, 1.0], 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert!((hits[0].score - 1.0).abs() < 1e-5);
    }

    #[test]
    fn top_k_by_cosine_orders_by_similarity_descending() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        seed_articles(&store, 4);
        store
            .batch_upsert_embeddings(&[
                (1, vec![1.0, 0.0], "m"),   // perfect match for query [1,0]
                (2, vec![0.71, 0.71], "m"), // 45° off → cos ≈ 0.71
                (3, vec![0.0, 1.0], "m"),   // orthogonal → cos = 0
                (4, vec![-1.0, 0.0], "m"),  // opposite → cos = -1
            ])
            .unwrap();
        let hits = store.top_k_by_cosine("m", &[1.0, 0.0], 4).unwrap();
        assert_eq!(hits.len(), 4);
        assert_eq!(hits[0].page_id, 1);
        assert!((hits[0].score - 1.0).abs() < 1e-5);
        assert_eq!(hits[1].page_id, 2);
        assert_eq!(hits[2].page_id, 3);
        assert_eq!(hits[3].page_id, 4);
    }

    #[test]
    fn top_k_caps_results() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        seed_articles(&store, 5);
        for i in 1..=5 {
            store
                .batch_upsert_embeddings(&[(i, vec![i as f32, 0.0], "m")])
                .unwrap();
        }
        let hits = store.top_k_by_cosine("m", &[1.0, 0.0], 2).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn top_k_with_zero_query_returns_empty() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        seed_articles(&store, 1);
        store
            .batch_upsert_embeddings(&[(1, vec![1.0, 1.0], "m")])
            .unwrap();
        let hits = store.top_k_by_cosine("m", &[0.0, 0.0], 5).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn top_k_skips_dim_mismatched_rows() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        seed_articles(&store, 2);
        store
            .batch_upsert_embeddings(&[
                (1, vec![1.0, 0.0, 0.0], "m"), // wrong dim — skipped
                (2, vec![1.0, 0.0], "m"),      // matches query dim
            ])
            .unwrap();
        let hits = store.top_k_by_cosine("m", &[1.0, 0.0], 5).unwrap();
        assert_eq!(hits.len(), 1, "dim-mismatched row must be skipped");
        assert_eq!(hits[0].page_id, 2);
    }

    #[test]
    fn articles_without_embedding_lists_only_unembedded() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        seed_articles(&store, 5);
        store
            .batch_upsert_embeddings(&[(1, vec![1.0, 0.0], "m"), (3, vec![0.5, 0.5], "m")])
            .unwrap();
        let pending = store.articles_without_embedding("m", 100).unwrap();
        let ids: Vec<u64> = pending.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![2, 4, 5]);

        // A different model has no embeddings at all → all 5 pending.
        let other = store.articles_without_embedding("other", 100).unwrap();
        assert_eq!(other.len(), 5);
    }

    #[test]
    fn embeddings_are_namespaced_by_model() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        seed_articles(&store, 1);
        store
            .batch_upsert_embeddings(&[(1, vec![1.0, 0.0], "model-a")])
            .unwrap();
        // Querying under a different model name returns nothing — the row
        // exists only under model-a.
        let hits = store.top_k_by_cosine("model-b", &[1.0, 0.0], 5).unwrap();
        assert!(hits.is_empty());
    }

    // --- Bookmark tests ---------------------------------------------------

    #[test]
    fn add_bookmark_round_trips() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let id = store
            .add_bookmark("Photon", None, Some("worth re-reading"))
            .unwrap();
        assert!(id > 0);
        assert!(store.is_bookmarked("Photon").unwrap());
        assert!(!store.is_bookmarked("Electron").unwrap());
        assert_eq!(store.count_bookmarks().unwrap(), 1);
    }

    #[test]
    fn add_bookmark_is_idempotent() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let a = store.add_bookmark("Photon", None, None).unwrap();
        let b = store.add_bookmark("Photon", None, None).unwrap();
        assert_eq!(a, b, "duplicate add must return existing id, not insert");
        assert_eq!(store.count_bookmarks().unwrap(), 1);
    }

    #[test]
    fn bookmark_in_different_folders_is_distinct() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let f1 = store.create_folder("Physics", None).unwrap();
        let f2 = store.create_folder("Reading list", None).unwrap();
        let a = store.add_bookmark("Photon", Some(f1), None).unwrap();
        let b = store.add_bookmark("Photon", Some(f2), None).unwrap();
        assert_ne!(a, b);
        assert_eq!(store.count_bookmarks().unwrap(), 2);
    }

    #[test]
    fn remove_bookmark_unfiles_or_drops() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let id = store.add_bookmark("Photon", None, None).unwrap();
        store.remove_bookmark(id).unwrap();
        assert!(!store.is_bookmarked("Photon").unwrap());
        // Idempotent — removing a missing id is fine.
        store.remove_bookmark(id).unwrap();
        store.remove_bookmark(99999).unwrap();
    }

    #[test]
    fn move_bookmark_between_folders() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let f1 = store.create_folder("F1", None).unwrap();
        let f2 = store.create_folder("F2", None).unwrap();
        let id = store.add_bookmark("Photon", Some(f1), None).unwrap();
        store.move_bookmark(id, Some(f2)).unwrap();
        assert_eq!(store.bookmarks_in_folder(Some(f1), 100).unwrap().len(), 0);
        assert_eq!(store.bookmarks_in_folder(Some(f2), 100).unwrap().len(), 1);
        // Move to root.
        store.move_bookmark(id, None).unwrap();
        assert_eq!(store.bookmarks_in_folder(None, 100).unwrap().len(), 1);
        assert_eq!(store.bookmarks_in_folder(Some(f2), 100).unwrap().len(), 0);
    }

    #[test]
    fn delete_folder_unfiles_its_bookmarks() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let f = store.create_folder("Doomed", None).unwrap();
        store.add_bookmark("Photon", Some(f), None).unwrap();
        store.add_bookmark("Electron", Some(f), None).unwrap();
        store.delete_folder(f).unwrap();
        // Folder is gone, but the bookmarks live on at root (folder_id = NULL).
        assert_eq!(store.list_folders().unwrap().len(), 0);
        assert_eq!(store.bookmarks_in_folder(None, 100).unwrap().len(), 2);
    }

    #[test]
    fn rename_folder() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        let f = store.create_folder("Old", None).unwrap();
        store.rename_folder(f, "New").unwrap();
        let folders = store.list_folders().unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "New");
    }

    #[test]
    fn list_folders_orders_by_name_case_insensitive() {
        let store = SqliteArticleStore::open_in_memory().unwrap();
        store.create_folder("zebra", None).unwrap();
        store.create_folder("Alpha", None).unwrap();
        store.create_folder("middle", None).unwrap();
        let names: Vec<String> = store
            .list_folders()
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, vec!["Alpha", "middle", "zebra"]);
    }
}
