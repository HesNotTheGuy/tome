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

/** Whether we're running inside a Tauri WebView vs. a browser-only dev session. */
export const IS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
