/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      // Tome design tokens, backed by CSS custom properties so a single
      // attribute change on <html> swaps the whole palette per preset.
      colors: {
        "tome-bg": "var(--tome-bg)",
        "tome-surface": "var(--tome-surface)",
        "tome-surface-2": "var(--tome-surface-2)",
        "tome-text": "var(--tome-text)",
        "tome-muted": "var(--tome-text-muted)",
        "tome-border": "var(--tome-border)",
        "tome-border-strong": "var(--tome-border-strong)",
        "tome-accent": "var(--tome-accent)",
        "tome-link": "var(--tome-link)",
        "tome-danger": "var(--tome-danger)",
        "tome-success": "var(--tome-success)",
      },
      fontFamily: {
        body: ["var(--tome-font-body)"],
        display: ["var(--tome-font-display)"],
        mono: ["var(--tome-font-mono)"],
      },
    },
  },
  plugins: [],
};
