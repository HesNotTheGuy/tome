import { useEffect, useRef, useState } from "react";
import { tome } from "../service";
import { IS_TAURI, SearchHit, Tier } from "../types";

interface SearchBarProps {
  onOpenArticle: (title: string) => void;
}

const ALL_TIERS: Tier[] = []; // empty = no tier filter

export default function SearchBar({ onOpenArticle }: SearchBarProps) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Cmd/Ctrl-K focus shortcut
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
      if (e.key === "Escape") {
        setOpen(false);
        inputRef.current?.blur();
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  // Click-outside to close.
  useEffect(() => {
    function onClickOutside(e: MouseEvent) {
      if (!containerRef.current?.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", onClickOutside);
    return () => document.removeEventListener("mousedown", onClickOutside);
  }, []);

  // Debounced query.
  useEffect(() => {
    if (query.trim().length < 2) {
      setHits([]);
      return;
    }
    if (!IS_TAURI) {
      setHits([
        {
          page_id: 0,
          title: `(demo) ${query}`,
          tier: "cold",
          score: 0,
        },
      ]);
      return;
    }
    let canceled = false;
    const handle = setTimeout(() => {
      tome
        .search(query.trim(), 10, ALL_TIERS)
        .then((r) => {
          if (!canceled) {
            setHits(r);
            setError(null);
          }
        })
        .catch((e) => {
          if (!canceled) {
            setError(String(e));
            setHits([]);
          }
        });
    }, 120);
    return () => {
      canceled = true;
      clearTimeout(handle);
    };
  }, [query]);

  function pick(hit: SearchHit) {
    onOpenArticle(hit.title);
    setOpen(false);
    setQuery("");
  }

  return (
    <div ref={containerRef} className="relative w-72 max-w-full">
      <input
        ref={inputRef}
        type="search"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        placeholder="Search… (⌘/Ctrl+K)"
        className="w-full px-3 py-1 text-sm rounded border border-zinc-300 dark:border-zinc-700 bg-white dark:bg-zinc-900 placeholder:text-zinc-400 focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
      {open && (query.trim().length >= 2 || error) && (
        <div className="absolute right-0 top-full mt-1 w-96 max-h-96 overflow-auto rounded-lg border border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 shadow-lg z-30">
          {error && (
            <div className="p-3 text-sm text-red-600 dark:text-red-400">
              {error}
            </div>
          )}
          {!error && hits.length === 0 && (
            <div className="p-3 text-sm text-zinc-500 dark:text-zinc-400">
              No matches.
            </div>
          )}
          <ul className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {hits.map((h) => (
              <li
                key={h.page_id}
                onMouseDown={(e) => {
                  // mousedown so the input doesn't lose focus first and
                  // dismiss the overlay before pick() runs.
                  e.preventDefault();
                  pick(h);
                }}
                className="p-3 hover:bg-zinc-50 dark:hover:bg-zinc-900 cursor-pointer"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium text-sm">{h.title}</span>
                  <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-zinc-100 dark:bg-zinc-800 text-zinc-500 dark:text-zinc-400">
                    {h.tier}
                  </span>
                </div>
                <p className="text-[11px] text-zinc-500 dark:text-zinc-500 mt-0.5">
                  score {h.score.toFixed(2)}
                </p>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
