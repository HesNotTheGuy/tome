import { useEffect, useRef, useState } from "react";
import { tome } from "../service";
import { EmbeddingHit, isTauri, SearchHit, Tier } from "../types";

interface SearchBarProps {
  onOpenArticle: (title: string) => void;
}

const ALL_TIERS: Tier[] = []; // empty = no tier filter

export default function SearchBar({ onOpenArticle }: SearchBarProps) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  // Semantic results are best-effort: if the user hasn't built the index,
  // or the embedder fails to load, we silently show nothing rather than
  // pollute the dropdown with errors. Lexical search is the primary surface.
  const [semanticHits, setSemanticHits] = useState<EmbeddingHit[]>([]);
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
      setSemanticHits([]);
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
      setSemanticHits([]);
      return;
    }
    let canceled = false;
    const handle = setTimeout(() => {
      const trimmed = query.trim();
      // Fire both searches in parallel. Lexical is the source of truth for
      // the dropdown's main list; semantic populates a "Related by meaning"
      // section below it. Each updates its own state on its own timeline so
      // a slow semantic call doesn't delay lexical hits showing up.
      tome
        .search(trimmed, 10, ALL_TIERS)
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
      tome
        .semanticSearch(trimmed, 8)
        .then((r) => {
          if (!canceled) setSemanticHits(r);
        })
        .catch(() => {
          // Silently drop. Common reasons: index not built, model not yet
          // downloaded, feature disabled at compile time. None worth
          // surfacing to a user mid-search.
          if (!canceled) setSemanticHits([]);
        });
    }, 120);
    return () => {
      canceled = true;
      clearTimeout(handle);
    };
  }, [query]);

  function pick(title: string) {
    onOpenArticle(title);
    setOpen(false);
    setQuery("");
  }

  // Drop semantic hits whose title already appears in the lexical results
  // — no point showing the same article twice. Comparison is
  // case-insensitive since lexical hits come back with the canonical
  // article title and embeddings were stored against that same title.
  const lexicalTitles = new Set(hits.map((h) => h.title.toLowerCase()));
  const dedupedSemantic = semanticHits.filter(
    (s) => !lexicalTitles.has(s.title.toLowerCase()),
  );

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
                  key={`lex-${h.page_id}`}
                  onMouseDown={(e) => {
                    // mousedown so the input doesn't lose focus first and
                    // dismiss the overlay before pick() runs.
                    e.preventDefault();
                    pick(h.title);
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

          {!error && dedupedSemantic.length > 0 && (
            <>
              <div className="px-3 py-1.5 text-[10px] uppercase tracking-wide text-tome-muted bg-tome-surface-2 border-y border-tome-border">
                Related by meaning
              </div>
              <ul className="divide-y divide-tome-border">
                {dedupedSemantic.map((s) => (
                  <li
                    key={`sem-${s.page_id}`}
                    onMouseDown={(e) => {
                      e.preventDefault();
                      pick(s.title);
                    }}
                    className="p-3 hover:bg-tome-surface-2 cursor-pointer"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium text-sm">{s.title}</span>
                      <span className="text-[10px] uppercase tracking-wide px-1.5 py-0.5 rounded bg-tome-surface-2 text-tome-muted">
                        ai
                      </span>
                    </div>
                    <p className="text-[11px] text-tome-muted mt-0.5">
                      similarity {s.score.toFixed(2)}
                    </p>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </div>
  );
}
