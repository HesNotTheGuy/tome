import { useEffect, useState } from "react";

import { tome } from "../service";
import { HistoryEntry, isTauri } from "../types";

interface HistoryProps {
  onOpen: (title: string) => void;
}

/**
 * History pane — recently-viewed articles.
 *
 * Pulls `recent_articles` from storage (rows where `last_accessed > 0`
 * ordered by recency desc). Click to re-open in the Reader. "Clear
 * history" resets the underlying counters workspace-wide; the off
 * switch in Settings stops new entries from being recorded going
 * forward but doesn't retroactively wipe.
 */
export default function History({ onOpen }: HistoryProps) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [enabled, setEnabled] = useState<boolean>(true);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [clearing, setClearing] = useState(false);

  function refresh() {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    Promise.all([tome.recentArticles(200), tome.historyEnabled()])
      .then(([rows, on]) => {
        setEntries(rows);
        setEnabled(on);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }

  useEffect(() => {
    refresh();
  }, []);

  async function handleClear() {
    if (!confirm("Clear all reading history? This cannot be undone.")) return;
    setClearing(true);
    try {
      await tome.clearHistory();
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setClearing(false);
    }
  }

  return (
    <section className="px-6 py-6 max-w-4xl mx-auto">
      <div className="mb-4 flex items-baseline justify-between gap-3">
        <div>
          <h2 className="text-2xl font-bold mb-1">History</h2>
          <p className="text-sm text-tome-muted">
            {enabled
              ? "Articles you've read recently. Newest first."
              : "History tracking is off — Settings → History to turn it back on."}
          </p>
        </div>
        {entries.length > 0 && (
          <button
            type="button"
            onClick={handleClear}
            disabled={clearing}
            className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2 disabled:opacity-50"
          >
            {clearing ? "Clearing…" : "Clear history"}
          </button>
        )}
      </div>

      {!isTauri() && (
        <div className="p-4 mb-4 rounded border border-tome-border bg-tome-surface-2 text-sm">
          Running outside the Tauri shell — no data available.
        </div>
      )}

      {error && (
        <div className="p-3 mb-3 rounded border border-tome-danger/50 bg-tome-danger/10 text-sm text-tome-danger">
          {error}
        </div>
      )}

      {loading && <div className="text-sm text-tome-muted">Loading…</div>}

      {!loading && entries.length === 0 && !error && (
        <div className="p-6 rounded border border-dashed border-tome-border text-center text-sm text-tome-muted">
          {enabled
            ? "No articles yet. Open one in the Reader and it'll appear here."
            : "History is off. No entries to show."}
        </div>
      )}

      {entries.length > 0 && (
        <ul className="rounded border border-tome-border overflow-hidden divide-y divide-tome-border">
          {entries.map((e) => (
            <li
              key={e.page_id}
              onClick={() => onOpen(e.title)}
              className="p-3 hover:bg-tome-surface-2 cursor-pointer flex items-center justify-between gap-3"
            >
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium truncate">{e.title}</div>
                <div className="text-xs text-tome-muted mt-0.5">
                  {formatTimestamp(e.last_accessed)}
                  {e.access_count > 1 && ` · read ${e.access_count} times`}
                </div>
              </div>
              <span className="text-xs text-tome-muted shrink-0">→</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** Human-readable relative timestamp ("3 minutes ago"). Falls back to
 *  a locale date string for anything older than a week. */
function formatTimestamp(unixSeconds: number): string {
  if (!unixSeconds) return "never";
  const now = Math.floor(Date.now() / 1000);
  const diff = now - unixSeconds;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} hr ago`;
  if (diff < 604800) return `${Math.floor(diff / 86400)} days ago`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}
