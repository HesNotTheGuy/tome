import { useEffect, useState } from "react";
import { tome } from "../service";
import { isTauri, SavedRevisionMeta } from "../types";

interface ArchiveProps {
  onOpen: (title: string) => void;
}

export default function Archive({ onOpen }: ArchiveProps) {
  const [items, setItems] = useState<SavedRevisionMeta[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!isTauri()) {
      setLoaded(true);
      return;
    }
    const promise =
      query.trim().length > 0
        ? tome.searchArchive(query, 50)
        : tome.listArchive();
    promise
      .then(setItems)
      .catch((e) => setError(String(e)))
      .finally(() => setLoaded(true));
  }, [query]);

  return (
    <section className="px-6 py-6 max-w-4xl mx-auto">
      <h2 className="text-2xl font-bold mb-1">Archive</h2>
      <p className="text-sm text-tome-muted mb-4">
        Permanently saved revisions. Searchable across notes, titles, and
        full content. Survives dump replacement.
      </p>

      <input
        type="search"
        placeholder="Search saved revisions…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="w-full px-3 py-2 mb-4 rounded border border-tome-border bg-tome-surface text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
      />

      {!isTauri() && (
        <div className="p-4 mb-4 rounded border border-tome-border bg-tome-surface-2 text-sm">
          Running outside the Tauri shell — backend not connected.
        </div>
      )}

      {error && (
        <div className="p-3 mb-4 rounded border border-tome-danger/50 bg-tome-danger/10 text-sm text-tome-danger">
          {error}
        </div>
      )}

      {loaded && items.length === 0 && !error && (
        <div className="p-6 rounded border border-dashed border-tome-border text-center text-sm text-tome-muted">
          {query.trim().length > 0
            ? "No saved revisions match that query."
            : "No saved revisions yet. Save one from the Reader's timeline."}
        </div>
      )}

      <ul className="divide-y divide-tome-border rounded border border-tome-border overflow-hidden">
        {items.map((rev) => (
          <li
            key={rev.id}
            className="p-3 hover:bg-tome-surface-2 cursor-pointer"
            onClick={() => onOpen(rev.title)}
          >
            <div className="flex items-center justify-between gap-2">
              <h3 className="font-medium text-sm text-tome-text">{rev.title}</h3>
              <span className="text-xs text-tome-muted">
                rev {rev.revision_id}
              </span>
            </div>
            {rev.user_note && (
              <p className="text-xs text-tome-muted mt-1 line-clamp-2">
                {rev.user_note}
              </p>
            )}
            <p className="text-[11px] text-tome-muted mt-1">
              saved {formatDate(rev.fetched_at)}
            </p>
          </li>
        ))}
      </ul>
    </section>
  );
}

function formatDate(unixSeconds: number): string {
  const d = new Date(unixSeconds * 1000);
  if (Number.isNaN(d.getTime())) return "unknown";
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
