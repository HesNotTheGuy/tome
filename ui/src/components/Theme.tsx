import { useEffect, useState } from "react";

const STORAGE_KEY = "tome.theme";

type Theme = "light" | "dark" | "system";

function effective(theme: Theme): "light" | "dark" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return theme;
}

function apply(theme: Theme) {
  const root = document.documentElement;
  if (effective(theme) === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
}

export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(() => {
    const stored = localStorage.getItem(STORAGE_KEY) as Theme | null;
    return stored ?? "system";
  });

  useEffect(() => {
    apply(theme);
    localStorage.setItem(STORAGE_KEY, theme);
  }, [theme]);

  // Re-apply when the OS theme changes and we're tracking system.
  useEffect(() => {
    if (theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => apply("system");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [theme]);

  return { theme, setTheme: setThemeState };
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
      className="px-2 py-1 rounded text-sm text-zinc-600 dark:text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-900"
    >
      {label[theme]}
    </button>
  );
}
