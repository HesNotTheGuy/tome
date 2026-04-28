import { useEffect, useState } from "react";

const THEME_KEY = "tome.theme";
const PRESET_KEY = "tome.preset";

type Theme = "light" | "dark" | "system";

export type Preset = "library" | "codex";

export const PRESETS: { id: Preset; label: string; tagline: string }[] = [
  {
    id: "library",
    label: "Library",
    tagline: "Neutral, polished — the default",
  },
  {
    id: "codex",
    label: "Codex",
    tagline: "Parchment serif — humanities",
  },
];

function effective(theme: Theme): "light" | "dark" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return theme;
}

function apply(theme: Theme, preset: Preset) {
  const root = document.documentElement;
  if (effective(theme) === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
  root.dataset.preset = preset;
}

let codexFontLoaded = false;
async function ensureCodexFont() {
  if (codexFontLoaded) return;
  codexFontLoaded = true;
  // Lazy-load the parchment serif only when Codex is selected, so Library
  // users don't pay for ~150 KB of font bytes they won't render.
  await Promise.all([
    import("@fontsource/crimson-pro/400.css"),
    import("@fontsource/crimson-pro/600.css"),
    import("@fontsource/crimson-pro/700.css"),
    import("@fontsource/crimson-pro/400-italic.css"),
  ]);
}

export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(() => {
    const stored = localStorage.getItem(THEME_KEY) as Theme | null;
    return stored ?? "system";
  });
  const [preset, setPresetState] = useState<Preset>(() => {
    const stored = localStorage.getItem(PRESET_KEY) as Preset | null;
    return stored ?? "library";
  });

  useEffect(() => {
    apply(theme, preset);
    localStorage.setItem(THEME_KEY, theme);
    localStorage.setItem(PRESET_KEY, preset);
    if (preset === "codex") {
      void ensureCodexFont();
    }
  }, [theme, preset]);

  // Re-apply when the OS theme changes and we're tracking system.
  useEffect(() => {
    if (theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => apply("system", preset);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [theme, preset]);

  return { theme, setTheme: setThemeState, preset, setPreset: setPresetState };
}

export function ThemeToggle() {
  const { theme, setTheme } = useTheme();
  const next: Record<Theme, Theme> = {
    light: "dark",
    dark: "system",
    system: "light",
  };
  const label: Record<Theme, string> = {
    light: "☀",
    dark: "☾",
    system: "◐",
  };
  return (
    <button
      type="button"
      onClick={() => setTheme(next[theme])}
      title={`Theme: ${theme} (click to cycle)`}
      className="px-2 py-1 rounded text-sm text-tome-muted hover:bg-tome-surface-2"
    >
      {label[theme]}
    </button>
  );
}

export function PresetPicker() {
  const { preset, setPreset } = useTheme();
  const [open, setOpen] = useState(false);

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        onBlur={() => setTimeout(() => setOpen(false), 100)}
        className="px-2 py-1 rounded text-sm text-tome-muted hover:bg-tome-surface-2 flex items-center gap-1"
        title="Visual preset"
      >
        {PRESETS.find((p) => p.id === preset)?.label ?? preset}
        <span className="text-[10px]">▾</span>
      </button>
      {open && (
        <div className="absolute right-0 top-full mt-1 w-56 rounded border border-tome-border bg-tome-surface shadow-lg z-30 overflow-hidden">
          {PRESETS.map((p) => (
            <button
              key={p.id}
              type="button"
              onMouseDown={(e) => {
                e.preventDefault();
                setPreset(p.id);
                setOpen(false);
              }}
              className={
                "w-full text-left px-3 py-2 hover:bg-tome-surface-2 " +
                (preset === p.id ? "bg-tome-surface-2" : "")
              }
            >
              <div className="text-sm font-medium">{p.label}</div>
              <div className="text-xs text-tome-muted">{p.tagline}</div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
