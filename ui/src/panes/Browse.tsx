import { useEffect, useState } from "react";
import { tome } from "../service";
import { CategoryMember, isTauri } from "../types";

interface BrowseProps {
  onOpen: (title: string) => void;
}

/**
 * Two-step browser: search for a category, then drill into its members.
 * No tree visualization for v1 — Wikipedia categories form a loose graph
 * with cycles, and a flat list-driven UI is more honest about that.
 */
export default function Browse({ onOpen }: BrowseProps) {
  const [activeCategory, setActiveCategory] = useState<string | null>(null);
  const [linkCount, setLinkCount] = useState<number | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    tome
      .countCategorylinks()
      .then(setLinkCount)
      .catch(() => setLinkCount(null));
  }, []);

  return (
    <section className="px-6 py-6 max-w-4xl mx-auto">
      <div className="mb-4">
        <h2 className="text-2xl font-bold mb-1">Browse</h2>
        <p className="text-sm text-tome-muted">
          Wikipedia&apos;s category tree. Search for a category, drill in to
          see articles and subcategories.
        </p>
      </div>

      {!isTauri() && (
        <div className="p-4 mb-4 rounded border border-tome-border bg-tome-surface-2 text-sm">
          Running outside the Tauri shell — no data available.
        </div>
      )}

      {isTauri() && linkCount !== null && linkCount === 0 && (
        <div className="p-6 rounded border border-dashed border-tome-border text-center text-sm text-tome-muted">
          No categorylinks ingested yet.
          <br />
          <span className="text-xs">
            Settings → Categorylinks ingestion → point at your downloaded{" "}
            <code className="px-1 py-0.5 bg-tome-surface-2 rounded">
              *-categorylinks.sql.gz
            </code>
            .
          </span>
        </div>
      )}

      {activeCategory ? (
        <CategoryView
          category={activeCategory}
          onBack={() => setActiveCategory(null)}
          onOpenArticle={onOpen}
          onPickSubcategory={setActiveCategory}
        />
      ) : (
        <CategorySearch onPick={setActiveCategory} />
      )}
    </section>
  );
}

function CategorySearch({ onPick }: { onPick: (cat: string) => void }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri() || query.trim().length < 2) {
      setResults([]);
      return;
    }
    let canceled = false;
    setLoading(true);
    const handle = setTimeout(() => {
      tome
        .searchCategories(query.trim(), 100)
        .then((r) => {
          if (!canceled) {
            setResults(r);
            setError(null);
          }
        })
        .catch((e) => {
          if (!canceled) setError(String(e));
        })
        .finally(() => {
          if (!canceled) setLoading(false);
        });
    }, 150);
    return () => {
      canceled = true;
      clearTimeout(handle);
    };
  }, [query]);

  return (
    <div>
      <input
        type="search"
        autoFocus
        placeholder="Search categories…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="w-full px-3 py-2 mb-4 rounded border border-tome-border bg-tome-surface text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
      />

      {error && (
        <div className="p-3 mb-3 rounded border border-tome-border text-sm text-tome-danger">
          {error}
        </div>
      )}

      {loading && (
        <div className="text-xs text-tome-muted">Searching…</div>
      )}

      {!loading && query.trim().length >= 2 && results.length === 0 && !error && (
        <div className="text-sm text-tome-muted py-3">
          No categories match.
        </div>
      )}

      <ul className="divide-y divide-tome-border rounded border border-tome-border overflow-hidden">
        {results.map((c) => (
          <li
            key={c}
            onClick={() => onPick(c)}
            className="p-3 hover:bg-tome-surface-2 cursor-pointer flex items-center justify-between"
          >
            <span className="text-sm">{c}</span>
            <span className="text-xs text-tome-muted">→</span>
          </li>
        ))}
      </ul>

      {query.trim().length < 2 && (
        <div className="p-6 text-center text-sm text-tome-muted">
          Type at least 2 characters to search.
        </div>
      )}
    </div>
  );
}

function CategoryView({
  category,
  onBack,
  onOpenArticle,
  onPickSubcategory,
}: {
  category: string;
  onBack: () => void;
  onOpenArticle: (title: string) => void;
  onPickSubcategory: (cat: string) => void;
}) {
  const [members, setMembers] = useState<CategoryMember[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    tome
      .categoryMembers(category, null, 200)
      .then((m) => {
        setMembers(m);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [category]);

  const pages = members.filter((m) => m.kind === "page");
  const subcats = members.filter((m) => m.kind === "subcat");
  const files = members.filter((m) => m.kind === "file");

  return (
    <div>
      <div className="flex items-center gap-3 mb-4">
        <button
          type="button"
          onClick={onBack}
          className="text-sm text-tome-muted hover:text-tome-text"
        >
          ← Back to search
        </button>
        <h3 className="text-lg font-semibold">{category}</h3>
      </div>

      {error && (
        <div className="p-3 mb-3 rounded border border-tome-border text-sm text-tome-danger">
          {error}
        </div>
      )}

      {loading && <div className="text-sm text-tome-muted">Loading…</div>}

      {!loading && members.length === 0 && !error && (
        <div className="p-6 rounded border border-dashed border-tome-border text-center text-sm text-tome-muted">
          This category has no members in the ingested data.
        </div>
      )}

      {subcats.length > 0 && (
        <Section
          title={`Subcategories (${subcats.length})`}
          items={subcats}
          onPick={(m) => onPickSubcategory(m.title)}
          icon="📁"
        />
      )}
      {pages.length > 0 && (
        <Section
          title={`Articles (${pages.length})`}
          items={pages}
          onPick={(m) => onOpenArticle(m.title)}
          icon="📄"
        />
      )}
      {files.length > 0 && (
        <Section
          title={`Files (${files.length})`}
          items={files}
          onPick={() => {
            /* file pages aren't readable as articles; no-op */
          }}
          icon="🗂"
          muted
        />
      )}
    </div>
  );
}

function Section({
  title,
  items,
  onPick,
  icon,
  muted = false,
}: {
  title: string;
  items: CategoryMember[];
  onPick: (m: CategoryMember) => void;
  icon: string;
  muted?: boolean;
}) {
  return (
    <div className="mb-6">
      <h4 className="text-xs font-semibold uppercase tracking-wide text-tome-muted mb-2">
        {title}
      </h4>
      <ul className="rounded border border-tome-border overflow-hidden divide-y divide-tome-border">
        {items.map((m) => (
          <li
            key={`${m.kind}-${m.page_id}-${m.title}`}
            onClick={() => onPick(m)}
            className={
              "p-3 flex items-center gap-2 hover:bg-tome-surface-2 " +
              (muted ? "cursor-default text-tome-muted" : "cursor-pointer")
            }
          >
            <span className="text-xs">{icon}</span>
            <span className="text-sm">{m.title}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
