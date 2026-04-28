// Service module — the only place the frontend touches the Tauri bridge.
//
// Every UI component goes through `tome` from this file rather than calling
// `invoke` directly. That keeps the contract between frontend and backend
// in one place and lets us mock cleanly in dev sessions outside a Tauri
// shell.

import {
  ArticleResponse,
  IngestSummary,
  InstalledModule,
  IS_TAURI,
  ModuleSpec,
  SavedRevisionMeta,
  SearchHit,
  Tier,
  TierCounts,
} from "./types";

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

let invoke: InvokeFn = async () => {
  throw new Error(
    "Tauri bridge not initialized — running outside a Tauri shell?",
  );
};

if (IS_TAURI) {
  // Lazy import so a browser-only dev session doesn't fail at module load.
  void import("@tauri-apps/api/core").then((mod) => {
    invoke = mod.invoke as InvokeFn;
  });
}

export interface TomeService {
  readArticle(title: string): Promise<ArticleResponse>;
  search(query: string, limit: number, tierFilter: Tier[]): Promise<SearchHit[]>;
  listModules(): Promise<InstalledModule[]>;
  installModule(spec: ModuleSpec, members: string[]): Promise<InstalledModule>;
  uninstallModule(id: string): Promise<void>;
  listArchive(): Promise<SavedRevisionMeta[]>;
  searchArchive(query: string, limit: number): Promise<SavedRevisionMeta[]>;
  saveRevision(args: {
    title: string;
    revisionId: number;
    wikitext: string;
    html?: string | null;
    userNote?: string | null;
  }): Promise<number>;
  killSwitchEngaged(): Promise<boolean>;
  setKillSwitch(engaged: boolean): Promise<void>;
  breakerOpen(): Promise<boolean>;
  userAgent(): Promise<string>;
  tierCounts(): Promise<TierCounts>;
  ingestIndex(
    path: string,
    onProgress: (count: number) => void,
  ): Promise<IngestSummary>;
  healthCheck(): Promise<string>;
}

/** Live service backed by Tauri commands. */
export const tome: TomeService = {
  readArticle(title) {
    return invoke<ArticleResponse>("read_article", { title });
  },
  search(query, limit, tierFilter) {
    return invoke<SearchHit[]>("search", { query, limit, tierFilter });
  },
  listModules() {
    return invoke<InstalledModule[]>("list_modules");
  },
  installModule(spec, members) {
    return invoke<InstalledModule>("install_module", { spec, members });
  },
  uninstallModule(id) {
    return invoke<void>("uninstall_module", { id });
  },
  listArchive() {
    return invoke<SavedRevisionMeta[]>("list_archive");
  },
  searchArchive(query, limit) {
    return invoke<SavedRevisionMeta[]>("search_archive", { query, limit });
  },
  saveRevision({ title, revisionId, wikitext, html, userNote }) {
    return invoke<number>("save_revision", {
      title,
      revisionId,
      wikitext,
      html: html ?? null,
      userNote: userNote ?? null,
    });
  },
  killSwitchEngaged() {
    return invoke<boolean>("kill_switch_engaged");
  },
  setKillSwitch(engaged) {
    return invoke<void>("set_kill_switch", { engaged });
  },
  breakerOpen() {
    return invoke<boolean>("breaker_open");
  },
  userAgent() {
    return invoke<string>("user_agent");
  },
  tierCounts() {
    return invoke<TierCounts>("tier_counts");
  },
  async ingestIndex(path, onProgress) {
    if (!IS_TAURI) {
      throw new Error("ingest requires the Tauri shell");
    }
    const eventMod = await import("@tauri-apps/api/event");
    const unlisten = await eventMod.listen<number>("ingest:progress", (e) => {
      onProgress(e.payload);
    });
    try {
      return await invoke<IngestSummary>("ingest_index", { path });
    } finally {
      unlisten();
    }
  },
  healthCheck() {
    return invoke<string>("health_check");
  },
};
