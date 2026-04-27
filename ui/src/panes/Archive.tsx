import { useEffect, useState } from "react";
import { tome } from "../service";
import { IS_TAURI, SavedRevisionMeta } from "../types";

interface ArchiveProps {
  onOpen: (title: string) => void;
}

export default function Archive({ onOpen }: ArchiveProps) {
  const [items, setItems] = useState<SavedRevisionMeta[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!IS_TAURI) {
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
      <p className="text-sm text-zinc-500 dark:text-zinc-400 mb-4">
        Permanently saved revisions. Searchable across notes, titles, and
        full content. Survives dump replacement.
      </p>

      <input
        type="search"
        placeholder="Search saved revisions…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="w-full px-3 py-2 mb-4 rounded border border-zinc-300 dark:border-zinc-700 bg-white dark:bg-zinc-900 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
      />

      {!IS_TAURI && (
        <div className="p-4 mb-4 rounded border border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-950 text-sm">
          Running outside the Tauri shell — backend not connected.
        </div>
      )}

      {error && (
        <div className="p-4 mb-4 rounded border border-red-300 dark:border-red-800 bg-red-50 dark:bg-red-950 text-sm text-red-700 dark:text-red-300">
          {error}
        </div>
      )}

      {loaded && items.length === 0 && !error && (
        <div className="p-6 rounded border border-dashed border-zinc-300 dark:border-zinc-700 text-center text-sm text-zinc-500 dark:text-zinc-400">
          {query.trim().length > 0
            ? "No saved revisions match that query."
            : "No saved revisions yet. Save one from the Reader's timeline."}
        </div>
      )}

      <ul className="divide-y divide-zinc-200 dark:divide-zinc-800 rounded border border-zinc-200 dark:border-zinc-800 overflow-hidden">
        {items.map((rev) => (
          <li
            key={rev.id}
            className="p-3 hover:bg-zinc-50 dark:hover:bg-zinc-900 cursor-pointer"
            onClick={() => onOpen(rev.title)}
          >
            <div className="flex items-center justify-between gap-2">
              <h3 className="font-medium text-sm">{rev.title}</h3>
              <span className="text-xs text-zinc-500 dark:text-zinc-400">
                rev {rev.revision_id}
              </span>
            </div>
            {rev.user_note && (
              <p className="text-xs text-zinc-600 dark:text-zinc-400 mt-1 line-clamp-2">
                {rev.user_note}
              </p>
            )}
            <p className="text-[11px] text-zinc-500 dark:text-zinc-500 mt-1">
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
