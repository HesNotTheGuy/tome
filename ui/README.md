# Tome UI

React + TypeScript + Vite + Tailwind frontend for Tome.

## Layout

```
ui/
├── src/
│   ├── main.tsx           # React entry, mounts <App/>
│   ├── App.tsx            # Three-pane shell (Library / Reader / Archive)
│   ├── service.ts         # Tauri-bridge wrapper — only file that imports @tauri-apps/api
│   ├── types.ts           # TypeScript mirrors of Rust serde types
│   ├── styles.css         # Tailwind base + .tome-article styles
│   ├── components/
│   │   └── Theme.tsx      # Light/dark/system toggle
│   └── panes/
│       ├── Library.tsx
│       ├── Reader.tsx
│       └── Archive.tsx
└── (configs: vite, ts, tailwind, postcss, eslint, prettier)
```

## Develop

```bash
npm install
npm run dev
```

Dev server runs at <http://localhost:1420>. The Tauri shell points its
WebView at this URL; running `npm run dev` standalone in a browser is fine
for UI iteration but the backend bridge will be unavailable (you'll see a
banner at the top of each pane).

For full app dev with the Rust backend connected:

```bash
# from the repo root
cargo tauri dev
```

## Type-check / lint / format

```bash
npm run typecheck
npm run lint
npm run format
```
