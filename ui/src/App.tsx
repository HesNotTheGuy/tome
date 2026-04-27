import { useEffect, useState } from "react";

import Library from "./panes/Library";
import Reader from "./panes/Reader";
import Archive from "./panes/Archive";
import Settings from "./panes/Settings";
import { ThemeToggle, useTheme } from "./components/Theme";
import SearchBar from "./components/SearchBar";

type Pane = "library" | "reader" | "archive" | "settings";

const PANE_LABEL: Record<Pane, string> = {
  library: "Library",
  reader: "Reader",
  archive: "Archive",
  settings: "Settings",
};

export default function App() {
  const [pane, setPane] = useState<Pane>("library");
  const [activeTitle, setActiveTitle] = useState<string | null>(null);
  useTheme(); // attaches dark-mode class to <html>

  // When the user opens an article from anywhere, jump to Reader.
  useEffect(() => {
    if (activeTitle) setPane("reader");
  }, [activeTitle]);

  return (
    <div className="h-full flex flex-col">
      <header className="flex items-center justify-between border-b border-zinc-200 dark:border-zinc-800 px-4 py-2 bg-white/60 dark:bg-zinc-950/60 backdrop-blur">
        <div className="flex items-center gap-3">
          <span className="font-bold text-lg tracking-tight">Tome</span>
          <span className="text-xs text-zinc-500 dark:text-zinc-400 hidden sm:inline">
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
                  ? "bg-zinc-200 dark:bg-zinc-800 text-zinc-900 dark:text-zinc-100"
                  : "text-zinc-600 dark:text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-900")
              }
            >
              {PANE_LABEL[p]}
            </button>
          ))}
        </nav>
        <div className="flex items-center gap-3">
          <SearchBar onOpenArticle={(title) => setActiveTitle(title)} />
          <ThemeToggle />
        </div>
      </header>

      <main className="flex-1 overflow-auto">
        {pane === "library" && <Library onOpen={(t) => setActiveTitle(t)} />}
        {pane === "reader" && (
          <Reader
            title={activeTitle}
            onNavigate={(t) => setActiveTitle(t)}
          />
        )}
        {pane === "archive" && <Archive onOpen={(t) => setActiveTitle(t)} />}
        {pane === "settings" && <Settings />}
      </main>

      <StatusBar />
    </div>
  );
}

function StatusBar() {
  return (
    <footer className="border-t border-zinc-200 dark:border-zinc-800 px-4 py-1 text-xs text-zinc-500 dark:text-zinc-400 flex items-center justify-between bg-white/60 dark:bg-zinc-950/60">
      <span>v0.1.0</span>
      <span>under construction</span>
    </footer>
  );
}
