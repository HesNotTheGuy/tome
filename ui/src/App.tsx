import { useEffect, useState } from "react";

import Library from "./panes/Library";
import Reader from "./panes/Reader";
import Archive from "./panes/Archive";
import Browse from "./panes/Browse";
import Settings from "./panes/Settings";
import { PresetPicker, ThemeToggle, useTheme } from "./components/Theme";
import SearchBar from "./components/SearchBar";

type Pane = "library" | "reader" | "browse" | "archive" | "settings";

const PANE_LABEL: Record<Pane, string> = {
  library: "Library",
  reader: "Reader",
  browse: "Browse",
  archive: "Archive",
  settings: "Settings",
};

export default function App() {
  const [pane, setPane] = useState<Pane>("library");
  const [activeTitle, setActiveTitle] = useState<string | null>(null);
  useTheme(); // attaches dark-mode class + data-preset to <html>

  useEffect(() => {
    if (activeTitle) setPane("reader");
  }, [activeTitle]);

  return (
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
          <SearchBar onOpenArticle={(title) => setActiveTitle(title)} />
          <PresetPicker />
          <ThemeToggle />
        </div>
      </header>

      <main className="flex-1 overflow-auto">
        {pane === "library" && <Library onOpen={(t) => setActiveTitle(t)} />}
        {pane === "reader" && (
          <Reader title={activeTitle} onNavigate={(t) => setActiveTitle(t)} />
        )}
        {pane === "browse" && <Browse onOpen={(t) => setActiveTitle(t)} />}
        {pane === "archive" && <Archive onOpen={(t) => setActiveTitle(t)} />}
        {pane === "settings" && <Settings />}
      </main>

      <StatusBar />
    </div>
  );
}

function StatusBar() {
  return (
    <footer
      className="border-t border-tome-border px-4 py-1 text-xs text-tome-muted flex items-center justify-between"
      style={{ backgroundColor: "color-mix(in srgb, var(--tome-surface) 60%, transparent)" }}
    >
      <span>v0.1.0</span>
      <span>under construction</span>
    </footer>
  );
}
