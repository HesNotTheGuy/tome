// TypeScript mirrors of the public types serialized across the Tauri bridge.
// Keep these in lockstep with the Rust serde definitions in
// `crates/tome-services` and `crates/tome-search`.

export type Tier = "hot" | "warm" | "cold" | "evicted";

export type ArticleSource =
  | "HotLocal"
  | "WarmLocal"
  | "DumpLocal"
  | "ApiCachedHtml";

export interface ArticleResponse {
  title: string;
  html: string;
  source: ArticleSource;
  revision_id: number | null;
}

export interface SearchHit {
  page_id: number;
  title: string;
  tier: Tier;
  score: number;
}

export interface CategorySpec {
  name: string;
  depth: number;
}

export interface ModuleSpec {
  id: string;
  name: string;
  description: string | null;
  default_tier: Tier;
  categories: CategorySpec[];
  explicit_titles: string[];
}

export interface InstalledModule {
  spec: ModuleSpec;
  member_count: number;
  installed_at: number;
  updated_at: number;
}

export interface SavedRevisionMeta {
  id: number;
  title: string;
  revision_id: number;
  fetched_at: number;
  user_note: string | null;
}

export interface SavedRevision extends SavedRevisionMeta {
  wikitext: string;
  html: string | null;
}

export interface TierCounts {
  hot: number;
  warm: number;
  cold: number;
  evicted: number;
}

export interface IngestSummary {
  entries_processed: number;
  elapsed_ms: number;
}

export interface Revision {
  revision_id: number;
  parent_id: number;
  minor: boolean;
  user: string;
  timestamp: string;
  size: number;
  comment: string;
}

export interface Geotag {
  page_id: number;
  lat: number;
  lon: number;
  primary: boolean;
  kind: string | null;
}

export interface MappedGeotag {
  page_id: number;
  title: string;
  lat: number;
  lon: number;
  kind: string | null;
}

export interface EmbeddingHit {
  page_id: number;
  title: string;
  /** Cosine similarity in [-1, 1]; higher is more similar. */
  score: number;
}

export interface EmbeddingIngestSummary {
  articles_embedded: number;
  elapsed_ms: number;
}

export interface ChatAnswer {
  answer: string;
  citations: number[];
}

export interface HistoryEntry {
  page_id: number;
  title: string;
  /** Unix epoch seconds. 0 means never read. */
  last_accessed: number;
  access_count: number;
}

export interface Bookmark {
  id: number;
  article_title: string;
  folder_id: number | null;
  note: string | null;
  created_at: number;
}

export interface BookmarkFolder {
  id: number;
  name: string;
  parent_id: number | null;
  created_at: number;
}

export interface BookmarkExportSummary {
  path: string;
  folders: number;
  bookmarks: number;
  format_version: number;
}

export interface BookmarkImportSummary {
  folders_created: number;
  folders_matched: number;
  bookmarks_added: number;
  bookmarks_skipped: number;
  source_format_version: number;
  /** True when the backup came from a newer Tome and was imported best-effort. */
  from_newer_version: boolean;
}

export interface DiskSpaceCheck {
  free_bytes: number;
  total_bytes: number;
  required_bytes: number;
  free_after_download_pct: number;
  recommended_min_pct: number;
  warn: boolean;
}

export interface GeotagSummary {
  entries_processed: number;
  elapsed_ms: number;
}

export type CategoryMemberKind = "page" | "subcat" | "file";

export interface CategoryMember {
  kind: CategoryMemberKind;
  title: string;
  page_id: number;
}

export interface CategoryIngestSummary {
  entries_processed: number;
  elapsed_ms: number;
}

export interface RedirectIngestSummary {
  entries_processed: number;
  elapsed_ms: number;
}

export interface RelatedArticle {
  page_id: number;
  title: string;
  shared_categories: number;
}

/**
 * Whether we're running inside a Tauri WebView vs. a browser-only dev session.
 *
 * Evaluated at *call time* rather than module load — Tauri 2 injects its
 * globals during page setup but our React bundle may evaluate first, so a
 * captured const would lock in the wrong answer.
 */
export function isTauri(): boolean {
  if (typeof window === "undefined") return false;
  // Tauri 2 (current) and Tauri 1 (fallback) globals.
  return "__TAURI_INTERNALS__" in window || "__TAURI__" in window;
}
