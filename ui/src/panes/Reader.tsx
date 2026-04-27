import { useEffect, useState } from "react";
import { tome } from "../service";
import { ArticleResponse, IS_TAURI } from "../types";

interface ReaderProps {
  title: string | null;
  onNavigate: (title: string) => void;
}

export default function Reader({ title, onNavigate }: ReaderProps) {
  const [response, setResponse] = useState<ArticleResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
    tome
      .readArticle(title)
      .then((r) => setResponse(r))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [title]);

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
      <div className="sticky top-0 z-10 bg-white/80 dark:bg-zinc-950/80 backdrop-blur border-b border-zinc-200 dark:border-zinc-800 px-6 py-3 max-w-3xl mx-auto flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold">{response?.title ?? title}</h1>
          {response?.source && (
            <p className="text-xs text-zinc-500 dark:text-zinc-400">
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
      </div>

      {loading && (
        <div className="px-6 py-6 max-w-3xl mx-auto text-sm text-zinc-500">
          Loading…
        </div>
      )}

      {error && (
        <div className="px-6 py-6 max-w-3xl mx-auto">
          <div className="p-4 rounded border border-red-300 dark:border-red-800 bg-red-50 dark:bg-red-950 text-sm text-red-700 dark:text-red-300">
            {error}
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
