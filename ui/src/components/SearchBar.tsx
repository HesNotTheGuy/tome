import { useEffect, useRef, useState } from "react";
import { tome } from "../service";
import { isTauri, SearchHit, Tier } from "../types";

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
    if (!isTauri()) {
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
        onKeyDown={(e) => {
          if (e.key === "Enter" && query.trim().length >= 2) {
            e.preventDefault();
            onOpenArticle(query.trim());
            setOpen(false);
            setQuery("");
          }
        }}
        placeholder="Search… (⌘/Ctrl+K)"
        className="w-full px-3 py-1 text-sm rounded border border-tome-border bg-tome-surface placeholder:text-tome-muted focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
      {open && (query.trim().length >= 2 || error) && (
        <div className="absolute right-0 top-full mt-1 w-96 max-h-96 overflow-auto rounded-lg border border-tome-border bg-tome-surface shadow-lg z-30">
          {/* "Open as X" entry — guaranteed path even when the search index */}
          {/* is empty or the query doesn't match anything indexed. */}
          {query.trim().length >= 2 && (
            <button
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                onOpenArticle(query.trim());
                setOpen(false);
                setQuery("");
              }}
              className="w-full text-left p-3 border-b border-tome-border hover:bg-tome-surface-2 cursor-pointer"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-sm">
                  Open as{" "}
                  <span className="font-medium">“{query.trim()}”</span>
                </span>
                <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-tome-surface-2 text-tome-muted">
                  ↵
                </span>
              </div>
              <p className="text-[11px] text-tome-muted mt-0.5">
                Resolves through Cold tier or the API
              </p>
            </button>
          )}

          {error && (
            <div className="p-3 text-sm text-tome-danger">{error}</div>
          )}
          {!error && hits.length > 0 && (
            <ul className="divide-y divide-tome-border">
              {hits.map((h) => (
                <li
                  key={h.page_id}
                  onMouseDown={(e) => {
                    // mousedown so the input doesn't lose focus first and
                    // dismiss the overlay before pick() runs.
                    e.preventDefault();
                    pick(h);
                  }}
                  className="p-3 hover:bg-tome-surface-2 cursor-pointer"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium text-sm">{h.title}</span>
                    <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-tome-surface-2 text-tome-muted">
                      {h.tier}
                    </span>
                  </div>
                  <p className="text-[11px] text-tome-muted mt-0.5">
                    score {h.score.toFixed(2)}
                  </p>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
