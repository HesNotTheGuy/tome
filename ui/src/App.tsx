import { useCallback, useEffect, useState } from "react";

import Library from "./panes/Library";
import Reader from "./panes/Reader";
import Archive from "./panes/Archive";
import Bookmarks from "./panes/Bookmarks";
import Browse from "./panes/Browse";
import History from "./panes/History";
import MapPane from "./panes/Map";
import Settings from "./panes/Settings";
import { PresetPicker, ThemeToggle, useTheme } from "./components/Theme";
import FirstRunWizard from "./components/FirstRunWizard";
import SearchBar from "./components/SearchBar";
import { DialogProvider } from "./components/Dialog";
import { tome } from "./service";
import { isTauri } from "./types";

type Pane =
  | "library"
  | "reader"
  | "browse"
  | "map"
  | "history"
  | "bookmarks"
  | "archive"
  | "settings";

const PANE_LABEL: Record<Pane, string> = {
  library: "Library",
  reader: "Reader",
  browse: "Browse",
  map: "Map",
  history: "History",
  bookmarks: "Bookmarks",
  archive: "Archive",
  settings: "Settings",
};

export default function App() {
  const [pane, setPane] = useState<Pane>("library");
  // Article navigation history — a linear back/forward stack like a browser.
  // `stack` is the visited titles, `index` the current position. Opening a
  // new article truncates any forward entries (standard browser semantics);
  // back/forward just move the index. Kept as one object so a push updates
  // both fields atomically.
  const [nav, setNav] = useState<{ stack: string[]; index: number }>({
    stack: [],
    index: -1,
  });
  const activeTitle = nav.index >= 0 ? nav.stack[nav.index]! : null;
  const canGoBack = nav.index > 0;
  const canGoForward = nav.index >= 0 && nav.index < nav.stack.length - 1;
  // Wizard visibility: undefined while we're checking, true/false after.
  // Avoids a flash of the empty Library before the wizard mounts.
  const [showWizard, setShowWizard] = useState<boolean | undefined>(undefined);
  useTheme(); // attaches dark-mode class + data-preset to <html>

  const navigate = useCallback((title: string) => {
    setNav((n) => {
      // Re-opening the current article is a no-op — no duplicate stack entry.
      if (n.index >= 0 && n.stack[n.index] === title) return n;
      const stack = n.stack.slice(0, n.index + 1);
      stack.push(title);
      return { stack, index: stack.length - 1 };
    });
  }, []);

  const goBack = useCallback(() => {
    setNav((n) => (n.index > 0 ? { ...n, index: n.index - 1 } : n));
  }, []);
  const goForward = useCallback(() => {
    setNav((n) =>
      n.index < n.stack.length - 1 ? { ...n, index: n.index + 1 } : n,
    );
  }, []);

  useEffect(() => {
    if (activeTitle) setPane("reader");
  }, [activeTitle]);

  useEffect(() => {
    // First-run heuristic: outside Tauri we never show the wizard (browser
    // dev mode). Inside Tauri, show it when the user has neither configured
    // a dump nor ingested any articles.
    if (!isTauri()) {
      setShowWizard(false);
      return;
    }
    (async () => {
      try {
        const [dump, counts] = await Promise.all([
          tome.dumpPath(),
          tome.tierCounts(),
        ]);
        const empty =
          counts.hot + counts.warm + counts.cold + counts.evicted === 0;
        setShowWizard(!dump && empty);
      } catch {
        // If anything fails we err on the side of NOT showing the wizard,
        // so an existing user with a transient backend hiccup isn't trapped
        // behind a setup screen.
        setShowWizard(false);
      }
    })();
  }, []);

  return (
    <DialogProvider>
      <div className="h-full flex flex-col">
        <header
          className="flex items-center justify-between border-b border-tome-border px-4 py-2 backdrop-blur"
          style={{ backgroundColor: "color-mix(in srgb, var(--tome-surface) 60%, transparent)" }}
        >
          <div className="flex items-center gap-3">
            <span className="font-bold text-lg tracking-tight">Tome</span>
            <span className="text-xs text-tome-muted hidden sm:inline">
              offline Wikipedia
            </span>
          </div>
          <nav className="flex items-center gap-1">
            {(Object.keys(PANE_LABEL) as Pane[]).map((p) => (
              <button
                key={p}
                type="button"
                onClick={() => setPane(p)}
                className={
                  "px-3 py-1 rounded text-sm transition-colors " +
                  (pane === p
                    ? "bg-tome-surface-2 text-tome-text"
                    : "text-tome-muted hover:bg-tome-surface-2")
                }
              >
                {PANE_LABEL[p]}
              </button>
            ))}
          </nav>
          <div className="flex items-center gap-2">
            <SearchBar onOpenArticle={navigate} />
            <RandomButton onOpen={navigate} />
            <PresetPicker />
            <ThemeToggle />
          </div>
        </header>

        <main className="flex-1 overflow-auto">
          {pane === "library" && <Library onOpen={navigate} />}
          {pane === "reader" && (
            <Reader
              title={activeTitle}
              onNavigate={navigate}
              onBack={goBack}
              onForward={goForward}
              canGoBack={canGoBack}
              canGoForward={canGoForward}
            />
          )}
          {pane === "browse" && <Browse onOpen={navigate} />}
          {pane === "map" && <MapPane onOpen={navigate} />}
          {pane === "history" && <History onOpen={navigate} />}
          {pane === "bookmarks" && <Bookmarks onOpen={navigate} />}
          {pane === "archive" && <Archive onOpen={navigate} />}
          {pane === "settings" && <Settings />}
        </main>

        <StatusBar />

        {showWizard && (
          <FirstRunWizard
            onComplete={() => {
              setShowWizard(false);
              setPane("library");
            }}
            onSkip={() => setShowWizard(false)}
          />
        )}
      </div>
    </DialogProvider>
  );
}

/**
 * Header button that picks a uniformly-random article from local storage
 * and opens it in the Reader. Disables itself when storage is empty (no
 * dump ingested yet) so a user without articles isn't given a useless
 * button. The dice icon is universally read as "random."
 */
function RandomButton({ onOpen }: { onOpen: (title: string) => void }) {
  const [busy, setBusy] = useState(false);
  const [empty, setEmpty] = useState<boolean | null>(null);

  // Check on mount whether storage has anything to randomize over.
  // Re-checking after every click would be paranoid; the user ingesting
  // a dump mid-session is rare enough to ignore.
  useEffect(() => {
    if (!isTauri()) {
      setEmpty(true);
      return;
    }
    tome
      .tierCounts()
      .then((c) => {
        const total = c.hot + c.warm + c.cold + c.evicted;
        setEmpty(total === 0);
      })
      .catch(() => setEmpty(null));
  }, []);

  async function pick() {
    if (!isTauri() || busy) return;
    setBusy(true);
    try {
      const title = await tome.randomArticle();
      if (title) onOpen(title);
    } catch {
      // Swallow — the button isn't load-bearing.
    } finally {
      setBusy(false);
    }
  }

  return (
    <button
      type="button"
      onClick={pick}
      disabled={busy || empty === true || empty === null}
      title={
        empty === true
          ? "Ingest a Wikipedia dump to enable"
          : "Open a random article"
      }
      aria-label="Open a random article"
      className="px-2 py-1 text-sm rounded text-tome-muted hover:bg-tome-surface-2 hover:text-tome-text disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
    >
      🎲
    </button>
  );
}

function StatusBar() {
  const [version, setVersion] = useState("");
  const [offline, setOffline] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    // Real bundled version from tauri.conf.json, not a hardcoded string.
    import("@tauri-apps/api/app")
      .then(({ getVersion }) => getVersion().then(setVersion))
      .catch(() => {});
    // Reflect offline mode, and keep it current if toggled in Settings.
    const sync = () => {
      tome
        .offlineMode()
        .then(setOffline)
        .catch(() => {});
    };
    sync();
    const id = setInterval(sync, 5000);
    return () => clearInterval(id);
  }, []);

  return (
    <footer
      className="border-t border-tome-border px-4 py-1 text-xs text-tome-muted flex items-center justify-between"
      style={{ backgroundColor: "color-mix(in srgb, var(--tome-surface) 60%, transparent)" }}
    >
      <span>{version ? `Tome v${version}` : "Tome"}</span>
      {offline && (
        <span className="text-tome-success" title="No network access — reading from local data only">
          ● Offline mode
        </span>
      )}
    </footer>
  );
}
