import { useEffect, useState } from "react";
import { tome } from "../service";
import PathField from "../components/PathField";
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
  offline: boolean;
  userAgent: string;
  tierCounts: TierCounts;
  recommendationsEnabled: boolean;
}

const EMPTY: SettingsState = {
  killSwitch: false,
  breakerOpen: false,
  offline: false,
  userAgent: "Tome/1.0 (+https://github.com/HesNotTheGuy/tome)",
  tierCounts: { hot: 0, warm: 0, cold: 0, evicted: 0 },
  recommendationsEnabled: true,
};

/** True when an ingest rejection was a user-initiated cancellation, not a real
 *  error. The backend maps TomeError::Cancelled to a string containing
 *  "cancelled"; we treat those neutrally instead of as a failure. */
function isCancellation(message: string): boolean {
  return message.toLowerCase().includes("cancelled");
}

export default function Settings() {
  const [state, setState] = useState<SettingsState>(EMPTY);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    if (!isTauri()) return;
    try {
      const [
        killSwitch,
        breakerOpen,
        offline,
        userAgent,
        tierCounts,
        recommendationsEnabled,
      ] = await Promise.all([
        tome.killSwitchEngaged(),
        tome.breakerOpen(),
        tome.offlineMode(),
        tome.userAgent(),
        tome.tierCounts(),
        tome.recommendationsEnabled(),
      ]);
      setState({
        killSwitch,
        breakerOpen,
        offline,
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

  async function toggleOffline() {
    if (!isTauri()) return;
    const next = !state.offline;
    try {
      await tome.setOfflineMode(next);
      // Offline mode drives the kill switch, so re-read both to keep the
      // lower-level "Block outbound traffic" display in sync.
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

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
      <p className="text-sm text-tome-muted mb-6">
        Network behavior and storage status. More controls land as features
        ship (dump path, ingestion, schedules, debug log).
      </p>

      {!isTauri() && (
        <div className="p-4 mb-6 rounded border border-tome-border bg-tome-surface-2 text-sm text-tome-muted">
          Running outside the Tauri shell — values are placeholders.
        </div>
      )}

      {error && (
        <div className="p-4 mb-6 rounded border border-tome-border bg-tome-surface text-sm text-tome-danger">
          {error}
        </div>
      )}

      <Section title="Internet access">
        <Row label="Offline mode">
          <button
            type="button"
            onClick={toggleOffline}
            disabled={!isTauri()}
            className={
              "px-3 py-1 text-sm rounded transition-colors " +
              (state.offline
                ? "text-white"
                : "bg-tome-bg text-tome-muted border border-tome-border hover:bg-tome-surface-2")
            }
            style={
              state.offline
                ? { backgroundColor: "var(--tome-success)" }
                : undefined
            }
          >
            {state.offline ? "ON — fully offline" : "click to turn on"}
          </button>
        </Row>
        <div className="px-4 py-3 text-xs text-tome-muted border-t border-tome-border">
          Block all internet access. Every article reads from your local data
          instantly — no network attempts, no waiting. Turn this on before you
          disconnect.
        </div>
        <Row label="Block outbound traffic">
          <button
            type="button"
            onClick={toggleKillSwitch}
            disabled={!isTauri()}
            className={
              "px-3 py-1 text-sm rounded transition-colors " +
              (state.killSwitch
                ? "text-white"
                : "bg-tome-surface-2 text-tome-text hover:bg-tome-border")
            }
            style={
              state.killSwitch
                ? { backgroundColor: "var(--tome-danger)" }
                : undefined
            }
          >
            {state.killSwitch ? "ENGAGED — outbound disabled" : "click to engage"}
          </button>
        </Row>
        <div className="px-4 py-3 text-xs text-tome-muted border-t border-tome-border">
          The lower-level switch behind offline mode. When engaged, Tome makes
          no outbound network calls at all. Turning offline mode on engages
          this automatically.
        </div>
        <Row label="Circuit breaker">
          <span
            className={
              "text-sm font-mono " +
              (state.breakerOpen ? "text-tome-danger" : "text-tome-success")
            }
          >
            {state.breakerOpen ? "OPEN (cooldown active)" : "closed"}
          </span>
        </Row>
        <div className="px-4 py-3 text-xs text-tome-muted border-t border-tome-border">
          Pauses network calls after repeated failures; clears itself
          automatically.
        </div>
        <Row label="User-Agent">
          <code className="text-xs text-tome-muted break-all">
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

      <HistoryToggleSection />

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
  const [docCount, setDocCount] = useState<number | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .dumpPath()
      .then((p) => {
        setStored(p);
        setDraft(p ?? "");
      })
      .catch((e) => setError(String(e)));
    tome
      .searchDocCount()
      .then(setDocCount)
      .catch(() => setDocCount(null));
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
        <PathField
          value={draft}
          onChange={setDraft}
          mode="openFile"
          filters={[{ name: "bzip2", extensions: ["bz2"] }]}
          disabled={phase === "saving"}
          placeholder="/path/to/enwiki-YYYYMMDD-pages-articles-multistream.xml.bz2"
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
        <div className="text-xs text-tome-muted border-t border-tome-border pt-3">
          {docCount === null ? (
            <span>—</span>
          ) : docCount > 0 ? (
            <span className="font-mono text-tome-text">
              {docCount.toLocaleString()}
            </span>
          ) : null}
          {docCount !== null &&
            (docCount > 0 ? (
              <span> titles searchable</span>
            ) : (
              <span>
                Search index empty — ingest the multistream index to enable
                search.
              </span>
            ))}
        </div>
      </div>
    </div>
  );
}

function IngestSection({ onComplete }: { onComplete: () => void }) {
  const [path, setPath] = useState("");
  const [phase, setPhase] = useState<
    "idle" | "running" | "done" | "error" | "cancelled"
  >("idle");
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
      const msg = String(e);
      if (isCancellation(msg)) {
        setError(null);
        setPhase("cancelled");
      } else {
        setError(msg);
        setPhase("error");
      }
    }
  }

  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-tome-muted mb-2">
        Dump ingestion
      </h3>
      <div className="rounded border border-tome-border bg-tome-surface p-4 space-y-3">
        <p className="text-xs text-tome-muted">
          Point Tome at a downloaded{" "}
          <code className="text-[11px] px-1 py-0.5 bg-tome-surface-2 rounded">
            *-multistream-index.txt.bz2
          </code>{" "}
          file. Tome streams the index and records each article&apos;s offset
          as Cold-tier metadata. The dump itself is read on-demand later.
        </p>
        <p className="text-xs text-tome-muted">
          Unlocks reading + title search. ~1–5 min.
        </p>

        <PathField
          value={path}
          onChange={setPath}
          mode="openFile"
          filters={[{ name: "bzip2", extensions: ["bz2"] }]}
          disabled={phase === "running"}
          placeholder="/path/to/enwiki-YYYYMMDD-pages-articles-multistream-index.txt.bz2"
        />

        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleIngest}
              disabled={phase === "running" || !isTauri()}
              className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
              style={{ backgroundColor: "var(--tome-accent)" }}
            >
              {phase === "running"
                ? `Ingesting… ${count.toLocaleString()} entries`
                : "Begin ingest"}
            </button>
            {phase === "running" && (
              <button
                type="button"
                onClick={() => tome.cancelIngest()}
                className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2"
              >
                Cancel
              </button>
            )}
          </div>
          {phase === "done" && summary && (
            <span className="text-xs text-tome-success">
              ✓ {summary.entries_processed.toLocaleString()} entries in{" "}
              {(summary.elapsed_ms / 1000).toFixed(1)}s
            </span>
          )}
          {phase === "cancelled" && (
            <span className="text-xs text-tome-muted">
              Cancelled — progress so far was kept
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

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-8">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-tome-muted mb-2">
        {title}
      </h3>
      <div className="rounded border border-tome-border bg-tome-surface divide-y divide-tome-border">
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
      <span className="text-sm text-tome-text">{label}</span>
      <div>{children}</div>
    </div>
  );
}

function CategorylinksSection() {
  const [path, setPath] = useState("");
  const [count, setCount] = useState<number | null>(null);
  const [phase, setPhase] = useState<
    "idle" | "running" | "done" | "error" | "cancelled"
  >("idle");
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
      const msg = String(e);
      if (isCancellation(msg)) {
        setError(null);
        setPhase("cancelled");
        const updated = await tome.countCategorylinks().catch(() => null);
        setCount(updated);
      } else {
        setError(msg);
        setPhase("error");
      }
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
        <p className="text-xs text-tome-muted">
          Unlocks the Browse pane and related-article suggestions. Can take
          30+ min on full enwiki. Keep the app open while it runs.
        </p>
        <div className="text-xs text-tome-muted">
          Currently stored:{" "}
          <span className="font-mono text-tome-text">
            {count?.toLocaleString() ?? "—"}
          </span>{" "}
          links
        </div>

        <PathField
          value={path}
          onChange={setPath}
          mode="openFile"
          filters={[{ name: "gzip", extensions: ["gz"] }]}
          disabled={phase === "running"}
          placeholder="/path/to/simplewiki-latest-categorylinks.sql.gz"
        />
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
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
            {phase === "running" && (
              <button
                type="button"
                onClick={() => tome.cancelIngest()}
                className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2"
              >
                Cancel
              </button>
            )}
          </div>
          {phase === "done" && summary && (
            <span className="text-xs text-tome-success">
              ✓ {summary.entries_processed.toLocaleString()} links in{" "}
              {(summary.elapsed_ms / 1000).toFixed(1)}s
            </span>
          )}
          {phase === "cancelled" && (
            <span className="text-xs text-tome-muted">
              Cancelled — progress so far was kept
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
  const [phase, setPhase] = useState<
    "idle" | "running" | "done" | "error" | "cancelled"
  >("idle");
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
      const msg = String(e);
      if (isCancellation(msg)) {
        setError(null);
        setPhase("cancelled");
        const updated = await tome.countRedirects().catch(() => null);
        setCount(updated);
      } else {
        setError(msg);
        setPhase("error");
      }
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
        <p className="text-xs text-tome-muted">
          Makes &quot;USA&quot; open &quot;United States&quot;. A few minutes.
        </p>
        <div className="text-xs text-tome-muted">
          Currently stored:{" "}
          <span className="font-mono text-tome-text">
            {count?.toLocaleString() ?? "—"}
          </span>{" "}
          redirects
        </div>

        <PathField
          value={path}
          onChange={setPath}
          mode="openFile"
          filters={[{ name: "gzip", extensions: ["gz"] }]}
          disabled={phase === "running"}
          placeholder="/path/to/simplewiki-latest-redirect.sql.gz"
        />
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
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
            {phase === "running" && (
              <button
                type="button"
                onClick={() => tome.cancelIngest()}
                className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2"
              >
                Cancel
              </button>
            )}
          </div>
          {phase === "done" && summary && (
            <span className="text-xs text-tome-success">
              ✓ {summary.entries_processed.toLocaleString()} redirects in{" "}
              {(summary.elapsed_ms / 1000).toFixed(1)}s
            </span>
          )}
          {phase === "cancelled" && (
            <span className="text-xs text-tome-muted">
              Cancelled — progress so far was kept
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
  const [phase, setPhase] = useState<
    "idle" | "running" | "done" | "error" | "cancelled"
  >("idle");
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
      const msg = String(e);
      if (isCancellation(msg)) {
        setError(null);
        setPhase("cancelled");
        const updated = await tome.countGeotags().catch(() => null);
        setCount(updated);
      } else {
        setError(msg);
        setPhase("error");
      }
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
        <p className="text-xs text-tome-muted">
          Unlocks map pins and article coordinates. A few minutes.
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

        <PathField
          value={path}
          onChange={setPath}
          mode="openFile"
          filters={[{ name: "gzip", extensions: ["gz"] }]}
          disabled={phase === "running"}
          placeholder="/path/to/simplewiki-latest-geo_tags.sql.gz"
        />
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
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
            {phase === "running" && (
              <button
                type="button"
                onClick={() => tome.cancelIngest()}
                className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2"
              >
                Cancel
              </button>
            )}
          </div>
          {phase === "done" && summary && (
            <span className="text-xs text-tome-success">
              ✓ {summary.entries_processed.toLocaleString()} geotags in{" "}
              {(summary.elapsed_ms / 1000).toFixed(1)}s
            </span>
          )}
          {phase === "cancelled" && (
            <span className="text-xs text-tome-muted">
              Cancelled — progress so far was kept
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

        <PathField
          value={path}
          onChange={setPath}
          mode="openFile"
          filters={[{ name: "PMTiles", extensions: ["pmtiles"] }]}
          disabled={phase === "saving"}
          placeholder="/path/to/world.pmtiles"
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
  const [phase, setPhase] = useState<
    "idle" | "checking" | "warning" | "downloading" | "done" | "error"
  >("idle");
  const [bytes, setBytes] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [diskCheck, setDiskCheck] = useState<import("../types").DiskSpaceCheck | null>(null);
  // Side-load: a path to a .gguf the user already has (e.g. on a USB drive).
  // When set, the downloader is unnecessary — Ask Tome reads from this file.
  const [sideloadPath, setSideloadPath] = useState<string | null>(null);
  const [sideloadDraft, setSideloadDraft] = useState("");
  const [sideloadPhase, setSideloadPhase] = useState<
    "idle" | "saving" | "saved" | "error"
  >("idle");
  const [sideloadError, setSideloadError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .chatModelPresent()
      .then(setPresent)
      .catch(() => setPresent(null));
    tome
      .chatModelPath()
      .then((p) => {
        setSideloadPath(p);
        setSideloadDraft(p ?? "");
      })
      .catch(() => setSideloadPath(null));
  }, []);

  async function saveSideload() {
    if (!isTauri()) return;
    setSideloadPhase("saving");
    setSideloadError(null);
    try {
      const next =
        sideloadDraft.trim().length > 0 ? sideloadDraft.trim() : null;
      await tome.setChatModelPath(next);
      setSideloadPath(next);
      setSideloadPhase("saved");
      setTimeout(() => setSideloadPhase("idle"), 2000);
    } catch (e) {
      setSideloadError(String(e));
      setSideloadPhase("error");
    }
  }

  async function clearSideload() {
    if (!isTauri()) return;
    setSideloadPhase("saving");
    setSideloadError(null);
    try {
      await tome.setChatModelPath(null);
      setSideloadPath(null);
      setSideloadDraft("");
      setSideloadPhase("saved");
      setTimeout(() => setSideloadPhase("idle"), 2000);
    } catch (e) {
      setSideloadError(String(e));
      setSideloadPhase("error");
    }
  }

  // Pre-flight: run the disk-space check, surface a warning modal if
  // it would leave the volume below the 15% threshold. Warn-only —
  // user can always click through.
  async function handleClickDownload() {
    if (!isTauri()) return;
    setPhase("checking");
    setError(null);
    try {
      const check = await tome.checkDiskSpaceForChatModel();
      setDiskCheck(check);
      if (check.warn) {
        setPhase("warning");
      } else {
        await startDownload();
      }
    } catch (e) {
      // If the check itself fails (rare — needs a real I/O fault),
      // fall through to the download. We'd rather attempt and fail
      // gracefully than block on a flaky stat call.
      setError(`disk check failed, continuing anyway: ${e}`);
      await startDownload();
    }
  }

  async function startDownload() {
    if (!isTauri()) return;
    setPhase("downloading");
    setBytes(0);
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
        {sideloadPath ? (
          <span className="text-xs text-tome-success">
            ✓ using side-loaded file
          </span>
        ) : (
          <>
            {present === null && (
              <span className="text-xs text-tome-muted">checking…</span>
            )}
            {present === true && (
              <span className="text-xs text-tome-success">✓ downloaded</span>
            )}
            {present === false && (
              <span className="text-xs text-tome-muted">not downloaded</span>
            )}
          </>
        )}
      </Row>
      <Row label="">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={handleClickDownload}
            disabled={
              phase === "downloading" ||
              phase === "checking" ||
              present === true ||
              sideloadPath !== null ||
              !isTauri()
            }
            className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ backgroundColor: "var(--tome-accent)" }}
          >
            {phase === "checking"
              ? "Checking…"
              : phase === "downloading"
                ? bytes > 0
                  ? `Downloading… ${(bytes / (1024 * 1024)).toFixed(1)} MB`
                  : "Downloading…"
                : sideloadPath
                  ? "Not needed — file side-loaded"
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
        One-time download from HuggingFace (needs internet). Once on disk,
        &quot;Ask Tome&quot; in the Reader will answer questions with
        citations to source articles — fully offline, no cloud calls. We run
        a quick disk-space check before starting and warn if the download
        would leave your drive uncomfortably full. No connection? Side-load
        the model file below instead.
      </div>

      <div className="px-4 py-3 border-t border-tome-border space-y-3">
        <p className="text-xs text-tome-muted">
          Already have the model file? Point Tome at it (e.g. on a USB drive)
          to use Ask Tome fully offline — no download needed.
        </p>
        <PathField
          value={sideloadDraft}
          onChange={setSideloadDraft}
          mode="openFile"
          filters={[{ name: "GGUF model", extensions: ["gguf"] }]}
          disabled={sideloadPhase === "saving" || !isTauri()}
          placeholder="/path/to/model.gguf"
        />
        <div className="flex items-center justify-between gap-3">
          <div className="flex gap-2">
            <button
              type="button"
              onClick={saveSideload}
              disabled={
                sideloadPhase === "saving" ||
                !isTauri() ||
                sideloadDraft.trim() === (sideloadPath ?? "")
              }
              className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
              style={{ backgroundColor: "var(--tome-accent)" }}
            >
              {sideloadPhase === "saving" ? "Saving…" : "Use this file"}
            </button>
            {sideloadPath && (
              <button
                type="button"
                onClick={clearSideload}
                disabled={sideloadPhase === "saving" || !isTauri()}
                className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2 disabled:opacity-50"
              >
                Clear
              </button>
            )}
          </div>
          <span className="text-xs text-tome-muted">
            {sideloadPhase === "saved" && (
              <span className="text-tome-success">✓ saved</span>
            )}
            {sideloadPhase === "error" && sideloadError && (
              <span className="text-tome-danger">{sideloadError}</span>
            )}
            {sideloadPhase === "idle" && sideloadPath && (
              <span>reverts to downloader on Clear</span>
            )}
          </span>
        </div>
      </div>

      {phase === "warning" && diskCheck && (
        <DiskSpaceWarning
          check={diskCheck}
          onCancel={() => setPhase("idle")}
          onContinue={() => {
            setPhase("idle");
            startDownload();
          }}
        />
      )}
    </Section>
  );
}

function DiskSpaceWarning({
  check,
  onCancel,
  onContinue,
}: {
  check: import("../types").DiskSpaceCheck;
  onCancel: () => void;
  onContinue: () => void;
}) {
  const freeGB = (check.free_bytes / 1e9).toFixed(1);
  const totalGB = (check.total_bytes / 1e9).toFixed(0);
  const requiredGB = (check.required_bytes / 1e9).toFixed(1);
  const wontFit = check.free_bytes < check.required_bytes;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="w-full max-w-md mx-4 rounded-lg border border-tome-border bg-tome-surface shadow-2xl overflow-hidden">
        <div className="px-5 py-3 border-b border-tome-border">
          <h3 className="text-lg font-bold">
            {wontFit ? "Not enough space" : "Drive will be tight"}
          </h3>
        </div>
        <div className="px-5 py-4 text-sm space-y-2">
          <p>
            Downloading this would use{" "}
            <span className="font-mono">{requiredGB} GB</span>, leaving{" "}
            <span
              className={
                "font-mono " +
                (wontFit ? "text-tome-danger" : "text-tome-text")
              }
            >
              {check.free_after_download_pct.toFixed(1)}%
            </span>{" "}
            free on a{" "}
            <span className="font-mono">{totalGB} GB</span> drive.
          </p>
          <p className="text-xs text-tome-muted">
            You have <span className="font-mono">{freeGB} GB</span> free
            right now. Most operating systems prefer at least{" "}
            <span className="font-mono">{check.recommended_min_pct}%</span>{" "}
            free for stable performance.
          </p>
          {wontFit && (
            <p className="text-xs text-tome-danger">
              Continuing will likely fail partway through; consider freeing
              space first.
            </p>
          )}
        </div>
        <div className="px-5 py-3 border-t border-tome-border flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="px-3 py-1.5 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onContinue}
            className="px-3 py-1.5 text-sm rounded text-white"
            style={{ backgroundColor: "var(--tome-accent)" }}
          >
            Download anyway
          </button>
        </div>
      </div>
    </div>
  );
}

function HistoryToggleSection() {
  const [enabled, setEnabled] = useState<boolean | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .historyEnabled()
      .then(setEnabled)
      .catch(() => setEnabled(null));
  }, []);

  async function toggle() {
    if (!isTauri() || enabled === null) return;
    const next = !enabled;
    try {
      await tome.setHistoryEnabled(next);
      setEnabled(next);
    } catch {
      // best-effort
    }
  }

  return (
    <Section title="History">
      <Row label="Track reading history">
        <button
          type="button"
          onClick={toggle}
          disabled={!isTauri() || enabled === null}
          className={
            "px-3 py-1 text-sm rounded transition-colors " +
            (enabled
              ? "bg-tome-surface-2 text-tome-text hover:bg-tome-border"
              : "bg-tome-bg text-tome-muted border border-tome-border hover:bg-tome-surface-2")
          }
        >
          {enabled === null ? "…" : enabled ? "on" : "off"}
        </button>
      </Row>
      <div className="px-4 py-3 text-xs text-tome-muted border-t border-tome-border">
        When on, opening an article in the Reader bumps its
        last-accessed timestamp so it shows up in the History pane.
        When off, no new history is recorded; existing entries stay
        until you Clear history from the History pane itself.
      </div>
    </Section>
  );
}
