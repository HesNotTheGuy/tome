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

If you fork Tome and ship a modified build, **change the `User-Agent` string** so MediaWiki and abuse reporters can identify your fork as the source of its traffic. The constant lives at [`crates/tome-config/src/lib.rs`](crates/tome-config/src/lib.rs) — `DEFAULT_USER_AGENT`. Use the format MediaWiki documents:

```
<your-fork-name>/<version> (+<your-contact-url>)
```

Concrete example: `Codex/1.2 (+https://github.com/you/codex)`. Don't keep `Tome/1.0` pointing at your fork's domain — it routes abuse reports to the wrong project. MediaWiki blocks misbehaving requests by IP, not by `User-Agent`, but the UA is how operators know who to talk to.

## License

[GPL-3.0](LICENSE) © HesNotTheGuy
