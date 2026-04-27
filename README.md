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

> The full build instructions arrive at step 9 (UI integration). For now, you can verify the Rust workspace compiles:

```bash
cargo check --workspace
cargo test --workspace
```

Requires Rust 1.85+ (edition 2024). Toolchain managed via `rust-toolchain.toml` once added.

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
