import { useEffect, useState } from "react";
import { tome } from "../service";
import { ArticleResponse, IS_TAURI, Revision } from "../types";
import Timeline from "../components/Timeline";

interface ReaderProps {
  title: string | null;
  onNavigate: (title: string) => void;
}

export default function Reader({ title, onNavigate }: ReaderProps) {
  const [response, setResponse] = useState<ArticleResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revisions, setRevisions] = useState<Revision[] | null>(null);
  const [revLoading, setRevLoading] = useState(false);
  const [revError, setRevError] = useState<string | null>(null);

  useEffect(() => {
    if (!title) {
      setResponse(null);
      return;
    }
    if (!IS_TAURI) {
      // Demo content when no backend is connected.
      setResponse({
        title,
        html: demoHtml(title),
        source: "DumpLocal",
        revision_id: null,
      });
      return;
    }
    setLoading(true);
    setError(null);
    setRevisions(null); // reset on article change
    setRevError(null);
    tome
      .readArticle(title)
      .then((r) => setResponse(r))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [title]);

  async function loadRevisions() {
    if (!title || !IS_TAURI) return;
    setRevLoading(true);
    setRevError(null);
    try {
      const list = await tome.fetchRevisions(title, 50);
      setRevisions(list);
    } catch (e) {
      setRevError(String(e));
    } finally {
      setRevLoading(false);
    }
  }

  // Internal links in the rendered HTML use `#/article/{slug}`. Catch them and
  // navigate without leaving the SPA.
  useEffect(() => {
    function onClick(e: MouseEvent) {
      const target = e.target as HTMLElement;
      const anchor = target.closest("a");
      if (!anchor) return;
      const href = anchor.getAttribute("href");
      if (!href) return;
      const m = href.match(/^#\/article\/(.+)$/);
      if (!m) return;
      e.preventDefault();
      const slug = decodeURIComponent(m[1] ?? "").replace(/_/g, " ");
      onNavigate(slug);
    }
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, [onNavigate]);

  if (!title) {
    return (
      <div className="px-6 py-10 max-w-3xl mx-auto text-center text-zinc-500 dark:text-zinc-400">
        <h2 className="text-xl font-semibold mb-2">No article open</h2>
        <p className="text-sm">
          Open one from the Library, or search the corpus from the search bar
          (Ctrl/⌘ + K, once it lands).
        </p>
      </div>
    );
  }

  return (
    <div className="relative">
      <div
        className="sticky top-0 z-10 backdrop-blur border-b border-tome-border px-6 py-3 max-w-3xl mx-auto flex items-start justify-between gap-4"
        style={{
          backgroundColor: "color-mix(in srgb, var(--tome-surface) 80%, transparent)",
        }}
      >
        <div className="flex-1">
          <h1 className="text-xl font-bold">{response?.title ?? title}</h1>
          {response?.source && (
            <p className="text-xs text-tome-muted">
              served from <code>{response.source}</code>
              {response.revision_id != null && (
                <>
                  {" "}
                  · rev <code>{response.revision_id}</code>
                </>
              )}
            </p>
          )}
        </div>
        <button
          type="button"
          onClick={loadRevisions}
          disabled={!IS_TAURI || revLoading}
          className="text-xs px-2 py-1 rounded border border-tome-border hover:bg-tome-surface-2 text-tome-muted disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {revLoading
            ? "Loading…"
            : revisions
              ? `Revisions · ${revisions.length}`
              : "Show revisions"}
        </button>
      </div>

      {loading && (
        <div className="px-6 py-6 max-w-3xl mx-auto text-sm text-tome-muted">
          Loading…
        </div>
      )}

      {error && (
        <div className="px-6 py-6 max-w-3xl mx-auto">
          <div className="p-4 rounded border border-tome-border bg-tome-surface-2 text-sm text-tome-danger">
            {error}
          </div>
        </div>
      )}

      {revisions && !revError && (
        <div className="px-6 py-4 max-w-3xl mx-auto border-b border-tome-border">
          <Timeline revisions={revisions} />
        </div>
      )}
      {revError && (
        <div className="px-6 py-2 max-w-3xl mx-auto">
          <div className="p-2 rounded border border-tome-border text-xs text-tome-danger">
            {revError}
          </div>
        </div>
      )}

      {response && !loading && !error && (
        <article
          className="tome-article"
          // Renderer output is escaped server-side; we trust our own backend.
          dangerouslySetInnerHTML={{ __html: response.html }}
        />
      )}
    </div>
  );
}

function demoHtml(title: string): string {
  return `<p>This is a placeholder for <strong>${escapeHtml(title)}</strong>. The Tauri backend isn't connected, so the renderer pipeline can't run. Launch via <code>cargo tauri dev</code> to read real articles.</p>`;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
