# Tome

Offline Wikipedia, with control. Tome reads Wikipedia's official multistream XML dumps and the live MediaWiki API to give you full local access to the entire English Wikipedia corpus, with granular control over what's stored locally and how much disk it uses.

> **Status:** under construction. This README will fill out as features land. See [ARCHITECTURE.md](ARCHITECTURE.md) for the module layout and data flows.

## What Tome does (target feature set)

- **Browse and search the full corpus**, even articles you haven't downloaded — Tome seeks into the dump file directly using a precomputed offset map.
- **Tiered storage** — articles live as Hot (instant), Warm (compressed), Cold (in the dump), or Evicted, individually or in bulk.
- **Modules** — install curated collections defined by Wikipedia categories ("Mathematics depth 3", "World History"), or roll your own.
- **Time machine** — fetch any article's revision history, view any historical version, diff two revisions, save permanent local copies of revisions you care about.
- **Updates** — refresh changed articles incrementally from the live API on demand or on a schedule.
- **Offline-first** — once content is downloaded, no internet required.

## Compatibility

### Operating systems

| OS | Status |
|---|---|
| **Windows 10 / 11 (x64)** | Tested. Primary target. WebView2 runtime auto-installs if absent. |
| **Windows 11 ARM** | Untested. Source build expected to work; not in CI. |
| **macOS (Apple Silicon, arm64)** | Built by CI, untested in practice. DMG published per release. |
| **macOS (Intel, x64)** | Built by CI, untested in practice. |
| **Linux x64** (Ubuntu 22.04+) | Built by CI, untested in practice. `.deb` and `AppImage` published. Needs `webkit2gtk-4.1` from your package manager. |
| **Linux ARM, BSDs, others** | Source-only — not in CI. |
| **iOS / Android** | Not supported. Tauri 2 has mobile in beta but Tome hasn't been adapted for it. |

### Hardware

| Component | Requirement |
|---|---|
| CPU | x86_64 with AVX. Pre-2011 CPUs lacking AVX won't run AI features but reader / search / map all work fine. |
| RAM | 4 GB minimum for reader-only. 8 GB if semantic search is enabled. 16 GB for the chat model. |
| Disk (app) | ~80 MB |
| Disk (your data) | 1 GB (Simple English) up to 30 GB (full English Wikipedia + AI models). Optional offline map files can add anywhere from 100 MB to 100 GB depending on coverage. |
| GPU | None required. Tome runs all AI on CPU by default; Metal / CUDA / Vulkan are auto-used if present. |

### Known conflicts and caveats

- **Antivirus false positives** on alpha builds. Until we have a code-signing certificate, Windows SmartScreen, Defender, and some third-party AVs will flag the installer as "unrecognized." Click "More info → Run anyway" to proceed. Same for macOS Gatekeeper: right-click the app and choose "Open" the first time.
- **Mac App Store / Microsoft Store distribution is incompatible with GPL-3.0** ([reasons](https://www.gnu.org/licenses/gpl-faq.html#AppStore)). Tome will only ever be distributed by direct download.
- **WebView differences** — Tome runs in WebView2 (Win), WKWebView (mac), and WebKitGTK (Linux). Subtle CSS/JS rendering differences exist; we don't currently test all three matrices.
- **Strictly offline once configured.** Tome talks to the network only for: live Wikipedia article HTML when an article isn't in your local dump, MediaWiki revision metadata when you open the timeline, and (if you opt in) HuggingFace for one-time model downloads. The Map pane has no online fallback at all — if you don't supply an offline `.pmtiles`, you see pins on a blank background.

## Going fully offline

Once prepared, every feature — reading, search, Browse, the Map, Ask Tome — works with no internet at all; flip on Offline mode in Settings → Network and Tome never touches it again. Preparation means gathering a handful of files while you still have a connection: the dump + index pair (required), three optional SQL tables, a `.pmtiles` basemap, and two AI models. The full checklist with sizes, download links, set-up order, and offline troubleshooting is in [docs/OFFLINE-SURVIVAL-KIT.md](docs/OFFLINE-SURVIVAL-KIT.md) — it's written to be printed and followed by someone non-technical, so it's the thing to hand to whoever you're setting a machine up for.

## Building

You'll need:

- Rust 1.85+ (edition 2024)
- Node.js 18+ and npm
- Platform build tools — Visual Studio Build Tools on Windows, Xcode CLT on macOS, `webkit2gtk` + `libssl-dev` on Linux

First-time setup:

```bash
# Rust workspace
cargo check --workspace

# Frontend deps
cd ui && npm install && cd ..
```

Run the dev shell (Tauri spawns Vite, opens a WebView pointed at it):

```bash
cargo install tauri-cli --version "^2"   # one-time
cargo tauri dev
```

Run the frontend in a browser without the Rust backend (faster iteration on UI styling):

```bash
cd ui && npm run dev      # http://localhost:1420
```

You'll see an "outside the Tauri shell" banner in each pane — that's expected; backend calls are stubbed.

### Optional: building with the local-LLM chat backend

Tome's "Ask Tome" feature uses [llama.cpp](https://github.com/ggerganov/llama.cpp) under the hood, wrapped via the [`llama-cpp-2`](https://crates.io/crates/llama-cpp-2) crate. That crate compiles llama.cpp from source during `cargo build`, which has additional prerequisites:

- **CMake** — drives llama.cpp's own build.
- **LLVM / libclang** — `llama-cpp-2` uses [`bindgen`](https://github.com/rust-lang/rust-bindgen) to generate Rust bindings from llama.cpp's C++ headers; `bindgen` requires `libclang` at build time.

Per-platform install:

```bash
# Windows (via Chocolatey)
choco install llvm cmake -y
# bindgen finds libclang automatically when LLVM is installed to its
# default path. If not, set LIBCLANG_PATH to the dir containing
# libclang.dll (typically `C:\Program Files\LLVM\bin`).

# macOS (via Homebrew)
brew install llvm cmake
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"

# Ubuntu / Debian
sudo apt-get install -y llvm-dev libclang-dev clang cmake
```

Then build with the feature enabled:

```bash
cargo tauri dev --features chat-inference
# or for a release artifact:
cargo tauri build --features chat-inference
```

The first compile takes ~5–10 minutes (llama.cpp itself compiles); subsequent builds use the cached artifacts.

**End-user note:** these prereqs are only for *building* Tome. People who install Tome from a release artifact never need LLVM, CMake, or any of this — those tools are baked into the published binaries by the [release workflow](.github/workflows/release.yml).

## Acquiring a Wikipedia dump

Tome works against Wikipedia's official multistream dumps. Download the latest from <https://dumps.wikimedia.org/enwiki/>:

- `enwiki-YYYYMMDD-pages-articles-multistream.xml.bz2` (the dump itself)
- `enwiki-YYYYMMDD-pages-articles-multistream-index.txt.bz2` (the companion index)

Detailed walkthrough lands in this README at the same time the ingestion UI does.

## Project layout

| Path | What's there |
|---|---|
| `crates/` | Rust library crates — one per architectural layer |
| `src-tauri/` | Tauri 2 desktop shell (added once UI integration begins) |
| `ui/` | React + TypeScript frontend (added once UI integration begins) |
| `samples/` | Example module definitions you can adapt |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Module dependency graph, layer responsibilities, data flows |

## API etiquette

Tome's MediaWiki API client enforces a 10 req/s ceiling, exponential backoff with `Retry-After` honored, and a circuit breaker that opens after 10 errors in a 60-second window. The default `User-Agent` is `Tome/1.0 (+https://github.com/HesNotTheGuy/tome)`, which is configurable in settings. If a Tome install ever appears to be misbehaving, please file an issue at <https://github.com/HesNotTheGuy/tome/issues> and we'll tighten the defaults.

## Forking

If you fork Tome and ship a modified build, **change the `User-Agent` string** so MediaWiki and abuse reporters can identify your fork as the source of its traffic. The constant lives at [`crates/tome-core/src/config.rs`](crates/tome-core/src/config.rs) — `DEFAULT_USER_AGENT`. Use the format MediaWiki documents:

```
<your-fork-name>/<version> (+<your-contact-url>)
```

Concrete example: `Codex/1.2 (+https://github.com/you/codex)`. Don't keep `Tome/1.0` pointing at your fork's domain — it routes abuse reports to the wrong project. MediaWiki blocks misbehaving requests by IP, not by `User-Agent`, but the UA is how operators know who to talk to.

## License

[GPL-3.0](LICENSE) © HesNotTheGuy
