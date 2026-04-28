import { useEffect, useState } from "react";
import { tome } from "../service";
import { IngestSummary, IS_TAURI, TierCounts } from "../types";

interface SettingsState {
  killSwitch: boolean;
  breakerOpen: boolean;
  userAgent: string;
  tierCounts: TierCounts;
}

const EMPTY: SettingsState = {
  killSwitch: false,
  breakerOpen: false,
  userAgent: "Tome/1.0 (+https://github.com/HesNotTheGuy/tome)",
  tierCounts: { hot: 0, warm: 0, cold: 0, evicted: 0 },
};

export default function Settings() {
  const [state, setState] = useState<SettingsState>(EMPTY);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    if (!IS_TAURI) return;
    try {
      const [killSwitch, breakerOpen, userAgent, tierCounts] = await Promise.all([
        tome.killSwitchEngaged(),
        tome.breakerOpen(),
        tome.userAgent(),
        tome.tierCounts(),
      ]);
      setState({ killSwitch, breakerOpen, userAgent, tierCounts });
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
    if (!IS_TAURI) return;
    const next = !state.killSwitch;
    try {
      await tome.setKillSwitch(next);
      setState((s) => ({ ...s, killSwitch: next }));
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

      {!IS_TAURI && (
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
            disabled={!IS_TAURI}
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


      <Section title="AI features (experimental)">
        <Row label="Master switch">
          <DisabledToggle label="off" />
        </Row>
        <Row label="Semantic search">
          <DisabledToggle label="not built" />
        </Row>
        <Row label="Ask Tome (RAG)">
          <DisabledToggle label="not built" />
        </Row>
        <div className="px-4 py-3 text-xs text-zinc-500 dark:text-zinc-400 border-t border-zinc-200 dark:border-zinc-800">
          Optional, opt-in, local-only. When enabled, downloads a small
          embedding model (~150 MB) for concept search and/or a quantized
          LLM (~2 GB) that answers with citations back to source articles.
          Both off by default; nothing runs until you turn it on.
        </div>
      </Section>
    </section>
  );
}

function DisabledToggle({ label }: { label: string }) {
  return (
    <button
      type="button"
      disabled
      title="Implementation pending — toggle wires up when the AI subsystem ships."
      className="px-3 py-1 text-sm rounded bg-zinc-100 dark:bg-zinc-800 text-zinc-400 dark:text-zinc-500 cursor-not-allowed"
    >
      {label}
    </button>
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

  async function handleIngest() {
    if (!IS_TAURI) {
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
            disabled={phase === "running" || !IS_TAURI}
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
