// Service module — the only place the frontend touches the Tauri bridge.
//
// Every UI component goes through `tome` from this file rather than calling
// `invoke` directly. That keeps the contract between frontend and backend
// in one place and lets us mock cleanly in dev sessions outside a Tauri
// shell.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";

import {
  ArticleResponse,
  IngestSummary,
  InstalledModule,
  ModuleSpec,
  Revision,
  SavedRevisionMeta,
  SearchHit,
  Tier,
  TierCounts,
} from "./types";

// `invoke` is statically imported. If we're in a browser context (no Tauri
// bridge), the underlying call throws with Tauri's own error string at the
// moment of invocation — there's no race with a lazy import.
const invoke = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
  tauriInvoke<T>(cmd, args);

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
  fetchRevisions(title: string, limit: number): Promise<Revision[]>;
  importModuleFromPath(path: string): Promise<InstalledModule>;
  dumpPath(): Promise<string | null>;
  setDumpPath(path: string | null): Promise<void>;
  lastIndexPath(): Promise<string | null>;
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
  fetchRevisions(title, limit) {
    return invoke<Revision[]>("fetch_revisions", { title, limit });
  },
  importModuleFromPath(path) {
    return invoke<InstalledModule>("import_module_from_path", { path });
  },
  dumpPath() {
    return invoke<string | null>("dump_path");
  },
  setDumpPath(path) {
    return invoke<void>("set_dump_path", { path });
  },
  lastIndexPath() {
    return invoke<string | null>("last_index_path");
  },
  async ingestIndex(path, onProgress) {
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
