import { useEffect, useState } from "react";
import { tome } from "../service";
import {
  CategoryIngestSummary,
  GeotagSummary,
  IngestSummary,
  isTauri,
  RedirectIngestSummary,
  TierCounts,
} from "../types";

interface SettingsState {
  killSwitch: boolean;
  breakerOpen: boolean;
  userAgent: string;
  tierCounts: TierCounts;
  recommendationsEnabled: boolean;
}

const EMPTY: SettingsState = {
  killSwitch: false,
  breakerOpen: false,
  userAgent: "Tome/1.0 (+https://github.com/HesNotTheGuy/tome)",
  tierCounts: { hot: 0, warm: 0, cold: 0, evicted: 0 },
  recommendationsEnabled: true,
};

export default function Settings() {
  const [state, setState] = useState<SettingsState>(EMPTY);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    if (!isTauri()) return;
    try {
      const [
        killSwitch,
        breakerOpen,
        userAgent,
        tierCounts,
        recommendationsEnabled,
      ] = await Promise.all([
        tome.killSwitchEngaged(),
        tome.breakerOpen(),
        tome.userAgent(),
        tome.tierCounts(),
        tome.recommendationsEnabled(),
      ]);
      setState({
        killSwitch,
        breakerOpen,
        userAgent,
        tierCounts,
        recommendationsEnabled,
      });
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 5_000);
    return () => clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function toggleKillSwitch() {
    if (!isTauri()) return;
    const next = !state.killSwitch;
    try {
      await tome.setKillSwitch(next);
      setState((s) => ({ ...s, killSwitch: next }));
    } catch (e) {
      setError(String(e));
    }
  }

  async function toggleRecommendations() {
    if (!isTauri()) return;
    const next = !state.recommendationsEnabled;
    try {
      await tome.setRecommendationsEnabled(next);
      setState((s) => ({ ...s, recommendationsEnabled: next }));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section className="px-6 py-6 max-w-3xl mx-auto">
      <h2 className="text-2xl font-bold mb-1">Settings</h2>
      <p className="text-sm text-zinc-500 dark:text-zinc-400 mb-6">
        Outbound API behavior and storage status. More controls land as
        features ship (dump path, ingestion, schedules, debug log).
      </p>

      {!isTauri() && (
        <div className="p-4 mb-6 rounded border border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-950 text-sm">
          Running outside the Tauri shell — values are placeholders.
        </div>
      )}

      {error && (
        <div className="p-4 mb-6 rounded border border-red-300 dark:border-red-800 bg-red-50 dark:bg-red-950 text-sm text-red-700 dark:text-red-300">
          {error}
        </div>
      )}

      <Section title="Outbound traffic">
        <Row label="Kill switch">
          <button
            type="button"
            onClick={toggleKillSwitch}
            disabled={!isTauri()}
            className={
              "px-3 py-1 text-sm rounded transition-colors " +
              (state.killSwitch
                ? "bg-red-600 text-white hover:bg-red-700"
                : "bg-zinc-200 dark:bg-zinc-800 text-zinc-700 dark:text-zinc-300 hover:bg-zinc-300 dark:hover:bg-zinc-700")
            }
          >
            {state.killSwitch ? "ENGAGED — outbound disabled" : "click to engage"}
          </button>
        </Row>
        <Row label="Circuit breaker">
          <span
            className={
              "text-sm font-mono " +
              (state.breakerOpen
                ? "text-red-600 dark:text-red-400"
                : "text-emerald-600 dark:text-emerald-400")
            }
          >
            {state.breakerOpen ? "OPEN (cooldown active)" : "closed"}
          </span>
        </Row>
        <Row label="User-Agent">
          <code className="text-xs text-zinc-600 dark:text-zinc-400 break-all">
            {state.userAgent}
          </code>
        </Row>
      </Section>

      <DumpPathSection />

      <Section title="Storage">
        <Row label="Hot tier">
          <span className="text-sm font-mono">
            {state.tierCounts.hot.toLocaleString()} articles
          </span>
        </Row>
        <Row label="Warm tier">
          <span className="text-sm font-mono">
            {state.tierCounts.warm.toLocaleString()} articles
          </span>
        </Row>
        <Row label="Cold tier">
          <span className="text-sm font-mono">
            {state.tierCounts.cold.toLocaleString()} articles
          </span>
        </Row>
        <Row label="Evicted">
          <span className="text-sm font-mono">
            {state.tierCounts.evicted.toLocaleString()} articles
          </span>
        </Row>
      </Section>

      <IngestSection onComplete={refresh} />

      <GeotagSection />

      <CategorylinksSection />

      <RedirectsSection />

      <MapSourceSection />

      <Section title="Reader behavior">
        <Row label="Show related articles">
          <button
            type="button"
            onClick={toggleRecommendations}
            disabled={!isTauri()}
            className={
              "px-3 py-1 text-sm rounded transition-colors " +
              (state.recommendationsEnabled
                ? "bg-tome-surface-2 text-tome-text hover:bg-tome-border"
                : "bg-tome-bg text-tome-muted border border-tome-border hover:bg-tome-surface-2")
            }
          >
            {state.recommendationsEnabled ? "on" : "off"}
          </button>
        </Row>
        <div className="px-4 py-3 text-xs text-tome-muted border-t border-tome-border">
          When on, the Reader shows up to 8 articles that share the most
          categories with what you&apos;re reading. Requires categorylinks to
          be ingested. Off saves an SQL lookup per article.
        </div>
      </Section>

      <SemanticSearchSection />

      <ChatModelSection />
    </section>
  );
}


function DumpPathSection() {
  const [stored, setStored] = useState<string | null>(null);
  const [draft, setDraft] = useState<string>("");
  const [phase, setPhase] = useState<"idle" | "saving" | "saved" | "error">(
    "idle",
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .dumpPath()
      .then((p) => {
        setStored(p);
        setDraft(p ?? "");
      })
      .catch((e) => setError(String(e)));
  }, []);

  async function save() {
    if (!isTauri()) return;
    setPhase("saving");
    setError(null);
    try {
      const next = draft.trim().length > 0 ? draft.trim() : null;
      await tome.setDumpPath(next);
      setStored(next);
      setPhase("saved");
      setTimeout(() => setPhase("idle"), 2000);
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  async function clear() {
    if (!isTauri()) return;
    setPhase("saving");
    setError(null);
    try {
      await tome.setDumpPath(null);
      setStored(null);
      setDraft("");
      setPhase("saved");
      setTimeout(() => setPhase("idle"), 2000);
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-tome-muted mb-2">
        Dump location
      </h3>
      <div className="rounded border border-tome-border bg-tome-surface p-4 space-y-3">
        <p className="text-xs text-tome-muted">
          Tome reads articles directly from your downloaded multistream bz2
          dump. Keep the file wherever you want — Tome only stores the path
          and reads bytes on demand. Required for Cold-tier reads (everything
          you haven&apos;t pulled into Hot or Warm).
        </p>
        <input
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          disabled={phase === "saving"}
          placeholder="/path/to/enwiki-YYYYMMDD-pages-articles-multistream.xml.bz2"
          className="w-full px-2 py-1 text-xs font-mono rounded border border-tome-border bg-tome-bg disabled:opacity-50"
        />
        <div className="flex items-center justify-between gap-3">
          <div className="flex gap-2">
            <button
              type="button"
              onClick={save}
              disabled={phase === "saving" || !isTauri() || draft.trim() === (stored ?? "")}
              className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
              style={{ backgroundColor: "var(--tome-accent)" }}
            >
              {phase === "saving" ? "Saving…" : "Save"}
            </button>
            {stored && (
              <button
                type="button"
                onClick={clear}
                disabled={phase === "saving" || !isTauri()}
                className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2 disabled:opacity-50"
              >
                Clear
              </button>
            )}
          </div>
          <span className="text-xs text-tome-muted">
            {phase === "saved" && (
              <span className="text-tome-success">✓ saved</span>
            )}
            {phase === "error" && error && (
              <span className="text-tome-danger">{error}</span>
            )}
            {phase === "idle" && stored && draft.trim() === stored && (
              <span>configured</span>
            )}
            {phase === "idle" && !stored && (
              <span>not configured — Cold reads will error</span>
            )}
          </span>
        </div>
      </div>
    </div>
  );
}

function IngestSection({ onComplete }: { onComplete: () => void }) {
  const [path, setPath] = useState("");
  const [phase, setPhase] = useState<"idle" | "running" | "done" | "error">(
    "idle",
  );
  const [count, setCount] = useState(0);
  const [summary, setSummary] = useState<IngestSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Pre-fill with the last index path the user ingested (if any), so they
  // don't have to retype the path on every launch.
  useEffect(() => {
    if (!isTauri()) return;
    tome
      .lastIndexPath()
      .then((p) => {
        if (p) setPath(p);
      })
      .catch(() => {
        /* non-fatal */
      });
  }, []);

  async function handleIngest() {
    if (!isTauri()) {
      setError("ingestion requires the Tauri shell");
      setPhase("error");
      return;
    }
    if (!path.trim()) {
      setError("paste the path to the multistream index file");
      setPhase("error");
      return;
    }
    setPhase("running");
    setCount(0);
    setError(null);
    setSummary(null);
    try {
      const result = await tome.ingestIndex(path.trim(), (n) => setCount(n));
      setSummary(result);
      setPhase("done");
      onComplete();
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-zinc-500 dark:text-zinc-400 mb-2">
        Dump ingestion
      </h3>
      <div className="rounded border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 p-4 space-y-3">
        <p className="text-xs text-zinc-500 dark:text-zinc-400">
          Point Tome at a downloaded{" "}
          <code className="text-[11px] px-1 py-0.5 bg-zinc-100 dark:bg-zinc-800 rounded">
            *-multistream-index.txt.bz2
          </code>{" "}
          file. Tome streams the index and records each article&apos;s offset
          as Cold-tier metadata. The dump itself is read on-demand later.
        </p>

        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          disabled={phase === "running"}
          placeholder="/path/to/enwiki-YYYYMMDD-pages-articles-multistream-index.txt.bz2"
          className="w-full px-2 py-1 text-xs font-mono rounded border border-zinc-300 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-950 disabled:opacity-50"
        />

        <div className="flex items-center justify-between gap-3">
          <button
            type="button"
            onClick={handleIngest}
            disabled={phase === "running" || !isTauri()}
            className="px-3 py-1 text-sm rounded bg-blue-600 text-white hover:bg-blue-700 disabled:bg-zinc-300 dark:disabled:bg-zinc-700 disabled:text-zinc-500 disabled:cursor-not-allowed"
          >
            {phase === "running"
              ? `Ingesting… ${count.toLocaleString()} entries`
              : "Begin ingest"}
          </button>
          {phase === "done" && summary && (
            <span className="text-xs text-emerald-700 dark:text-emerald-400">
              ✓ {summary.entries_processed.toLocaleString()} entries in{" "}
              {(summary.elapsed_ms / 1000).toFixed(1)}s
            </span>
          )}
          {phase === "error" && error && (
            <span className="text-xs text-red-600 dark:text-red-400">
              {error}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-zinc-500 dark:text-zinc-400 mb-2">
        {title}
      </h3>
      <div className="rounded border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900 divide-y divide-zinc-200 dark:divide-zinc-800">
        {children}
      </div>
    </div>
  );
}

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3">
      <span className="text-sm text-zinc-700 dark:text-zinc-300">{label}</span>
      <div>{children}</div>
    </div>
  );
}

function CategorylinksSection() {
  const [path, setPath] = useState("");
  const [count, setCount] = useState<number | null>(null);
  const [phase, setPhase] = useState<"idle" | "running" | "done" | "error">(
    "idle",
  );
  const [progress, setProgress] = useState(0);
  const [summary, setSummary] = useState<CategoryIngestSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .countCategorylinks()
      .then(setCount)
      .catch(() => setCount(null));
  }, []);

  async function handleIngest() {
    if (!isTauri()) {
      setError("requires the Tauri shell");
      setPhase("error");
      return;
    }
    if (!path.trim()) {
      setError("paste the path to a categorylinks.sql.gz file");
      setPhase("error");
      return;
    }
    setPhase("running");
    setProgress(0);
    setError(null);
    setSummary(null);
    try {
      const result = await tome.ingestCategorylinks(path.trim(), (n) =>
        setProgress(n),
      );
      setSummary(result);
      setPhase("done");
      const updated = await tome.countCategorylinks();
      setCount(updated);
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-tome-muted mb-2">
        Categorylinks ingestion
      </h3>
      <div className="rounded border border-tome-border bg-tome-surface p-4 space-y-3">
        <p className="text-xs text-tome-muted">
          Optional. Point Tome at a downloaded{" "}
          <code className="text-[11px] px-1 py-0.5 bg-tome-surface-2 rounded">
            *-categorylinks.sql.gz
          </code>{" "}
          to enable category browsing. ~28 MB simplewiki, ~2.4 GB enwiki.
          The Browse pane appears once any categorylinks are ingested.
        </p>
        <div className="text-xs text-tome-muted">
          Currently stored:{" "}
          <span className="font-mono text-tome-text">
            {count?.toLocaleString() ?? "—"}
          </span>{" "}
          links
        </div>

        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          disabled={phase === "running"}
          placeholder="/path/to/simplewiki-latest-categorylinks.sql.gz"
          className="w-full px-2 py-1 text-xs font-mono rounded border border-tome-border bg-tome-bg disabled:opacity-50"
        />
        <div className="flex items-center justify-between gap-3">
          <button
            type="button"
            onClick={handleIngest}
            disabled={phase === "running" || !isTauri()}
            className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ backgroundColor: "var(--tome-accent)" }}
          >
            {phase === "running"
              ? `Ingesting… ${progress.toLocaleString()}`
              : "Ingest categorylinks"}
          </button>
          {phase === "done" && summary && (
            <span className="text-xs text-tome-success">
              ✓ {summary.entries_processed.toLocaleString()} links in{" "}
              {(summary.elapsed_ms / 1000).toFixed(1)}s
            </span>
          )}
          {phase === "error" && error && (
            <span className="text-xs text-tome-danger">{error}</span>
          )}
        </div>
      </div>
    </div>
  );
}

function RedirectsSection() {
  const [path, setPath] = useState("");
  const [count, setCount] = useState<number | null>(null);
  const [phase, setPhase] = useState<"idle" | "running" | "done" | "error">(
    "idle",
  );
  const [progress, setProgress] = useState(0);
  const [summary, setSummary] = useState<RedirectIngestSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .countRedirects()
      .then(setCount)
      .catch(() => setCount(null));
  }, []);

  async function handleIngest() {
    if (!isTauri()) {
      setError("requires the Tauri shell");
      setPhase("error");
      return;
    }
    if (!path.trim()) {
      setError("paste the path to a redirect.sql.gz file");
      setPhase("error");
      return;
    }
    setPhase("running");
    setProgress(0);
    setError(null);
    setSummary(null);
    try {
      const result = await tome.ingestRedirects(path.trim(), (n) =>
        setProgress(n),
      );
      setSummary(result);
      setPhase("done");
      const updated = await tome.countRedirects();
      setCount(updated);
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-tome-muted mb-2">
        Redirects ingestion
      </h3>
      <div className="rounded border border-tome-border bg-tome-surface p-4 space-y-3">
        <p className="text-xs text-tome-muted">
          Optional. Point Tome at a downloaded{" "}
          <code className="text-[11px] px-1 py-0.5 bg-tome-surface-2 rounded">
            *-redirect.sql.gz
          </code>{" "}
          so that typing &quot;USA&quot; lands on &quot;United States&quot;.
          ~1 MB simplewiki, ~250 MB enwiki.
        </p>
        <div className="text-xs text-tome-muted">
          Currently stored:{" "}
          <span className="font-mono text-tome-text">
            {count?.toLocaleString() ?? "—"}
          </span>{" "}
          redirects
        </div>

        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          disabled={phase === "running"}
          placeholder="/path/to/simplewiki-latest-redirect.sql.gz"
          className="w-full px-2 py-1 text-xs font-mono rounded border border-tome-border bg-tome-bg disabled:opacity-50"
        />
        <div className="flex items-center justify-between gap-3">
          <button
            type="button"
            onClick={handleIngest}
            disabled={phase === "running" || !isTauri()}
            className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ backgroundColor: "var(--tome-accent)" }}
          >
            {phase === "running"
              ? `Ingesting… ${progress.toLocaleString()}`
              : "Ingest redirects"}
          </button>
          {phase === "done" && summary && (
            <span className="text-xs text-tome-success">
              ✓ {summary.entries_processed.toLocaleString()} redirects in{" "}
              {(summary.elapsed_ms / 1000).toFixed(1)}s
            </span>
          )}
          {phase === "error" && error && (
            <span className="text-xs text-tome-danger">{error}</span>
          )}
        </div>
      </div>
    </div>
  );
}

function GeotagSection() {
  const [path, setPath] = useState("");
  const [count, setCount] = useState<number | null>(null);
  const [phase, setPhase] = useState<"idle" | "running" | "done" | "error">(
    "idle",
  );
  const [progress, setProgress] = useState(0);
  const [summary, setSummary] = useState<GeotagSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome.countGeotags().then(setCount).catch(() => setCount(null));
  }, []);

  async function handleIngest() {
    if (!isTauri()) {
      setError("requires the Tauri shell");
      setPhase("error");
      return;
    }
    if (!path.trim()) {
      setError("paste the path to a geo_tags.sql.gz file");
      setPhase("error");
      return;
    }
    setPhase("running");
    setProgress(0);
    setError(null);
    setSummary(null);
    try {
      const result = await tome.ingestGeotags(path.trim(), (n) =>
        setProgress(n),
      );
      setSummary(result);
      setPhase("done");
      const updated = await tome.countGeotags();
      setCount(updated);
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-tome-muted mb-2">
        Geotag ingestion
      </h3>
      <div className="rounded border border-tome-border bg-tome-surface p-4 space-y-3">
        <p className="text-xs text-tome-muted">
          Optional. Point Tome at a downloaded{" "}
          <code className="text-[11px] px-1 py-0.5 bg-tome-surface-2 rounded">
            *-geo_tags.sql.gz
          </code>{" "}
          to attach geographic coordinates to articles. Tiny file (~1 MB
          simple, ~50 MB enwiki). Once ingested, the Reader shows
          coordinates on geographic articles.
        </p>
        <div className="flex items-center justify-between text-xs text-tome-muted">
          <span>
            Currently stored:{" "}
            <span className="font-mono text-tome-text">
              {count?.toLocaleString() ?? "—"}
            </span>{" "}
            geotags
          </span>
        </div>

        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          disabled={phase === "running"}
          placeholder="/path/to/simplewiki-latest-geo_tags.sql.gz"
          className="w-full px-2 py-1 text-xs font-mono rounded border border-tome-border bg-tome-bg disabled:opacity-50"
        />
        <div className="flex items-center justify-between gap-3">
          <button
            type="button"
            onClick={handleIngest}
            disabled={phase === "running" || !isTauri()}
            className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ backgroundColor: "var(--tome-accent)" }}
          >
            {phase === "running"
              ? `Ingesting… ${progress.toLocaleString()}`
              : "Ingest geotags"}
          </button>
          {phase === "done" && summary && (
            <span className="text-xs text-tome-success">
              ✓ {summary.entries_processed.toLocaleString()} geotags in{" "}
              {(summary.elapsed_ms / 1000).toFixed(1)}s
            </span>
          )}
          {phase === "error" && error && (
            <span className="text-xs text-tome-danger">{error}</span>
          )}
        </div>
      </div>
    </div>
  );
}

function MapSourceSection() {
  const [path, setPath] = useState("");
  const [saved, setSaved] = useState<string | null>(null);
  const [phase, setPhase] = useState<"idle" | "saving" | "saved" | "error">(
    "idle",
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .mapSourcePath()
      .then((p) => {
        setSaved(p);
        if (p) setPath(p);
      })
      .catch(() => setSaved(null));
  }, []);

  async function handleSave() {
    if (!isTauri()) return;
    setPhase("saving");
    setError(null);
    try {
      const trimmed = path.trim();
      await tome.setMapSourcePath(trimmed === "" ? null : trimmed);
      setSaved(trimmed === "" ? null : trimmed);
      setPhase("saved");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  async function handleClear() {
    setPath("");
    if (!isTauri()) return;
    try {
      await tome.setMapSourcePath(null);
      setSaved(null);
      setPhase("saved");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-tome-muted mb-2">
        Offline map source
      </h3>
      <div className="rounded border border-tome-border bg-tome-surface p-4 space-y-3">
        <p className="text-xs text-tome-muted">
          Optional. Point Tome at a{" "}
          <code className="text-[11px] px-1 py-0.5 bg-tome-surface-2 rounded">
            .pmtiles
          </code>{" "}
          file and the Map pane renders fully offline from it. Free regional
          and worldwide downloads are at{" "}
          <a
            href="https://maps.protomaps.com/builds/"
            className="underline text-tome-link"
          >
            maps.protomaps.com/builds/
          </a>
          . Without this, the Map pane shows pins on a blank background — no
          online fallback (Tome is strictly offline-first).
        </p>
        <div className="text-xs text-tome-muted">
          Currently configured:{" "}
          <span className="font-mono text-tome-text">
            {saved ? saved : "—"}
          </span>
        </div>

        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          disabled={phase === "saving"}
          placeholder="/path/to/world.pmtiles"
          className="w-full px-2 py-1 text-xs font-mono rounded border border-tome-border bg-tome-bg disabled:opacity-50"
        />
        <div className="flex items-center justify-between gap-3">
          <div className="flex gap-2">
            <button
              type="button"
              onClick={handleSave}
              disabled={phase === "saving" || !isTauri()}
              className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
              style={{ backgroundColor: "var(--tome-accent)" }}
            >
              {phase === "saving" ? "Saving…" : "Save"}
            </button>
            <button
              type="button"
              onClick={handleClear}
              disabled={!saved || !isTauri()}
              className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted disabled:opacity-50 disabled:cursor-not-allowed hover:bg-tome-surface-2"
            >
              Clear
            </button>
          </div>
          {phase === "saved" && (
            <span className="text-xs text-tome-success">✓ saved</span>
          )}
          {phase === "error" && error && (
            <span className="text-xs text-tome-danger">{error}</span>
          )}
        </div>
      </div>
    </div>
  );
}

function SemanticSearchSection() {
  const [count, setCount] = useState<number | null>(null);
  const [phase, setPhase] = useState<"idle" | "embedding" | "done" | "error">(
    "idle",
  );
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .countEmbeddings()
      .then(setCount)
      .catch(() => setCount(null));
  }, []);

  async function handleEmbed() {
    if (!isTauri()) return;
    setPhase("embedding");
    setProgress(0);
    setError(null);
    try {
      // 0 means "embed everything pending" — the loop terminates when the
      // articles_without_embedding query returns an empty page.
      await tome.embedArticles(0, (n) => setProgress(n));
      setPhase("done");
      const updated = await tome.countEmbeddings();
      setCount(updated);
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <Section title="Semantic search (AI)">
      <Row label="Articles embedded">
        <span className="text-sm font-mono">
          {count?.toLocaleString() ?? "—"}
        </span>
      </Row>
      <Row label="">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={handleEmbed}
            disabled={phase === "embedding" || !isTauri()}
            className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ backgroundColor: "var(--tome-accent)" }}
          >
            {phase === "embedding"
              ? `Embedding… ${progress.toLocaleString()}`
              : count && count > 0
                ? "Embed remaining"
                : "Build index"}
          </button>
          {phase === "done" && (
            <span className="text-xs text-tome-success">
              ✓ done · {count?.toLocaleString()} total
            </span>
          )}
          {phase === "error" && error && (
            <span className="text-xs text-tome-danger">{error}</span>
          )}
        </div>
      </Row>
      <div className="px-4 py-3 text-xs text-tome-muted border-t border-tome-border">
        Embeds article titles using BGE-small-en-v1.5 (~33 MB, downloads
        on first run from HuggingFace). Once built, semantic search finds
        articles by meaning — typing &quot;tools to navigate offline&quot;
        will surface compass and map articles even when those words don&apos;t
        appear in the title. Resumable — interrupting and re-running picks
        up where it left off.
      </div>
    </Section>
  );
}

function ChatModelSection() {
  const [present, setPresent] = useState<boolean | null>(null);
  const [phase, setPhase] = useState<"idle" | "downloading" | "done" | "error">(
    "idle",
  );
  const [bytes, setBytes] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .chatModelPresent()
      .then(setPresent)
      .catch(() => setPresent(null));
  }, []);

  async function handleDownload() {
    if (!isTauri()) return;
    setPhase("downloading");
    setBytes(0);
    setError(null);
    try {
      await tome.downloadChatModel((n) => setBytes(n));
      setPhase("done");
      setPresent(true);
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <Section title="Ask Tome (chat model)">
      <Row label="Model">
        <span className="text-xs font-mono text-tome-muted">
          microsoft/Phi-4-mini-instruct (Q4_K_M, ~2.3 GB)
        </span>
      </Row>
      <Row label="Status">
        {present === null && (
          <span className="text-xs text-tome-muted">checking…</span>
        )}
        {present === true && (
          <span className="text-xs text-tome-success">✓ downloaded</span>
        )}
        {present === false && (
          <span className="text-xs text-tome-muted">not downloaded</span>
        )}
      </Row>
      <Row label="">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={handleDownload}
            disabled={phase === "downloading" || present === true || !isTauri()}
            className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ backgroundColor: "var(--tome-accent)" }}
          >
            {phase === "downloading"
              ? bytes > 0
                ? `Downloading… ${(bytes / (1024 * 1024)).toFixed(1)} MB`
                : "Downloading…"
              : present
                ? "Already on disk"
                : "Download"}
          </button>
          {phase === "error" && error && (
            <span className="text-xs text-tome-danger">{error}</span>
          )}
        </div>
      </Row>
      <div className="px-4 py-3 text-xs text-tome-muted border-t border-tome-border">
        One-time download from HuggingFace. Once on disk, &quot;Ask Tome&quot;
        in the Reader will answer questions with citations to source
        articles — fully offline, no cloud calls. The chat backend itself
        ships in a follow-up commit; this lets you start the download
        ahead of time.
      </div>
    </Section>
  );
}
