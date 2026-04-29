import { useEffect, useState } from "react";
import { tome } from "../service";
import {
  ArticleResponse,
  Geotag,
  isTauri,
  RelatedArticle,
  Revision,
} from "../types";
import AskTome from "../components/AskTome";
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
  const [geotag, setGeotag] = useState<Geotag | null>(null);
  const [related, setRelated] = useState<RelatedArticle[]>([]);

  // Look up the geotag for the current article (if any) when the title
  // changes. Silent failures — if we have no geotags ingested or the
  // article has none, just don't render the coords badge.
  useEffect(() => {
    if (!title || !isTauri()) {
      setGeotag(null);
      return;
    }
    let canceled = false;
    tome
      .geotagForTitle(title)
      .then((g) => {
        if (!canceled) setGeotag(g);
      })
      .catch(() => {
        if (!canceled) setGeotag(null);
      });
    return () => {
      canceled = true;
    };
  }, [title]);

  // Recommendations: silently empty when disabled in settings or when no
  // categorylinks have been ingested.
  useEffect(() => {
    if (!title || !isTauri()) {
      setRelated([]);
      return;
    }
    let canceled = false;
    tome
      .relatedToTitle(title, 8)
      .then((r) => {
        if (!canceled) setRelated(r);
      })
      .catch(() => {
        if (!canceled) setRelated([]);
      });
    return () => {
      canceled = true;
    };
  }, [title]);

  useEffect(() => {
    if (!title) {
      setResponse(null);
      return;
    }
    if (!isTauri()) {
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
    if (!title || !isTauri()) return;
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

  // Comprehensive link interception. Wikipedia's API HTML uses several
  // different formats; we route every recognized pattern to the Reader and
  // open every other http(s) link in the system browser via Tauri's shell
  // plugin. **We always preventDefault on anchor clicks within the article**
  // so the WebView never silently navigates away from our app. (A backend
  // navigation guard is the second line of defense.)
  useEffect(() => {
    async function onClick(e: MouseEvent) {
      const target = e.target as HTMLElement;
      const anchor = target.closest("a");
      if (!anchor) return;

      // Don't intercept clicks outside the article container — we want the
      // search dropdown, nav buttons, and theme toggle to behave normally.
      if (!anchor.closest(".tome-article, .tome-link-handler")) return;

      const href = anchor.getAttribute("href");
      if (!href) return;

      // In-page anchor (e.g., #section-foo) — let the browser scroll, no
      // hijack.
      if (href.startsWith("#") && !href.startsWith("#/")) return;

      e.preventDefault();
      e.stopPropagation();

      const target_title = articleTitleFromHref(href);
      if (target_title) {
        onNavigate(target_title);
        return;
      }

      // External link — open in the user's default browser, never inside
      // our WebView.
      if (/^(https?:)?\/\//.test(href)) {
        try {
          const { open } = await import("@tauri-apps/plugin-shell");
          // Make protocol-relative URLs absolute.
          const url = href.startsWith("//") ? `https:${href}` : href;
          await open(url);
        } catch {
          /* shell plugin not available — fail silently rather than crash */
        }
      }
      // Anything else (mailto:, javascript:, etc.) is silently ignored.
    }
    document.addEventListener("click", onClick, true);
    return () => document.removeEventListener("click", onClick, true);
  }, [onNavigate]);

  if (!title) {
    return (
      <div className="px-6 py-10 max-w-3xl mx-auto text-center text-tome-muted">
        <h2 className="text-xl font-semibold mb-2 text-tome-text">No article open</h2>
        <p className="text-sm">
          Type any article title in the search bar (Ctrl/⌘ + K) and hit Enter.
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
        <div className="flex-1 min-w-0">
          <h1 className="text-xl font-bold text-tome-text truncate">
            {response?.title ?? title}
          </h1>
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
          {geotag && <CoordsBadge geotag={geotag} />}
        </div>
        <button
          type="button"
          onClick={loadRevisions}
          disabled={!isTauri() || revLoading}
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
        <>
          <article
            className="tome-article"
            // Renderer output is escaped server-side; we trust our own backend.
            dangerouslySetInnerHTML={{ __html: response.html }}
          />
          {related.length > 0 && (
            <RelatedSection items={related} onOpen={onNavigate} />
          )}
        </>
      )}
      <AskTome articleTitle={response?.title ?? title} onOpenArticle={onNavigate} />
    </div>
  );
}

function RelatedSection({
  items,
  onOpen,
}: {
  items: RelatedArticle[];
  onOpen: (title: string) => void;
}) {
  return (
    <section className="max-w-3xl mx-auto px-4 py-6 border-t border-tome-border">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-tome-muted mb-3">
        Related articles
      </h2>
      <ul className="grid gap-2 grid-cols-1 sm:grid-cols-2">
        {items.map((r) => (
          <li
            key={r.page_id}
            onClick={() => onOpen(r.title)}
            className="p-3 rounded border border-tome-border hover:bg-tome-surface-2 cursor-pointer flex items-center justify-between gap-3"
          >
            <span className="text-sm">{r.title}</span>
            <span className="text-[10px] text-tome-muted">
              {r.shared_categories} shared
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/**
 * Extract a Wikipedia article title from an `<a href>` value. Returns null
 * if the link is to anything other than an article on Wikipedia.
 *
 * Recognized patterns:
 *   - `#/article/Photon`                 (our local renderer's format)
 *   - `/wiki/Photon`                     (Wikipedia relative)
 *   - `//en.wikipedia.org/wiki/Photon`   (Wikipedia protocol-relative)
 *   - `https://en.wikipedia.org/wiki/X`  (Wikipedia absolute)
 *   - `./Photon`                         (Parsoid's relative-to-page form)
 *
 * Strips trailing fragments (#section). Decodes percent-encoding. Converts
 * underscores to spaces (Wikipedia URL convention).
 */
function articleTitleFromHref(href: string): string | null {
  // Our own format
  let m = href.match(/^#\/article\/([^?]+)$/);
  if (m) return cleanTitle(m[1]!);

  // Strip query and fragment for the rest.
  const cleaned = href.split("#")[0]!.split("?")[0]!;

  // Parsoid relative form: ./Photon
  m = cleaned.match(/^\.\/(.+)$/);
  if (m) return cleanTitle(m[1]!);

  // Wikipedia URL forms (relative, protocol-relative, absolute, any language).
  m = cleaned.match(
    /^(?:https?:)?\/\/[a-z-]+\.(?:m\.)?wikipedia\.org\/wiki\/(.+)$/i,
  );
  if (m) return cleanTitle(m[1]!);

  // Pure relative wiki path
  m = cleaned.match(/^\/wiki\/(.+)$/);
  if (m) return cleanTitle(m[1]!);

  return null;
}

function cleanTitle(raw: string): string {
  return decodeURIComponent(raw).replace(/_/g, " ");
}

function CoordsBadge({ geotag }: { geotag: Geotag }) {
  // Format as compact DMS-ish: "42.50° N · 71.00° W"
  const lat = `${Math.abs(geotag.lat).toFixed(2)}° ${geotag.lat >= 0 ? "N" : "S"}`;
  const lon = `${Math.abs(geotag.lon).toFixed(2)}° ${geotag.lon >= 0 ? "E" : "W"}`;
  // Also produce an OSM URL the user can click to open the location in
  // their default browser.
  const osm = `https://www.openstreetmap.org/?mlat=${geotag.lat}&mlon=${geotag.lon}#map=10/${geotag.lat}/${geotag.lon}`;
  return (
    <p className="text-xs text-tome-muted mt-1 tome-link-handler">
      📍{" "}
      <a
        href={osm}
        title="Open in OpenStreetMap"
        className="text-tome-link hover:underline"
      >
        <code>
          {lat} · {lon}
        </code>
      </a>
      {geotag.kind && (
        <span className="ml-2 px-1.5 py-0.5 rounded bg-tome-surface-2 text-[10px] uppercase tracking-wide">
          {geotag.kind}
        </span>
      )}
    </p>
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
