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
