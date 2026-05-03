# Tome — Architecture

## Module layout and dependency graph

```
                          ┌─────────────────┐
                          │       UI        │  (React + TS, src-tauri shell)
                          │  (added step 9) │
                          └────────┬────────┘
                                   │  Tauri commands
                                   ▼
                          ┌─────────────────┐
                          │  tome-services  │  thin orchestration; the
                          │                 │  only crate the UI may call
                          └─┬──┬──┬──┬──┬──┬┘
            ┌───────────────┘  │  │  │  │  └────────────┐
            ▼                  ▼  ▼  ▼  ▼               ▼
   ┌──────────────┐   ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
   │  tome-dump   │   │ tome-storage │ │   tome-api   │ │ tome-modules │
   │              │   │              │ │  (gatekeeper)│ │              │
   └──────┬───────┘   └──────┬───────┘ └──────┬───────┘ └──────┬───────┘
          │                  │                │                │
          │   ┌──────────────┼────────────────┘                │
          │   │              │                                 │
          ▼   ▼              ▼                                 │
   ┌──────────────┐   ┌──────────────┐                         │
   │  tome-search │   │tome-wikitext │                         │
   │              │   │              │                         │
   └──────────────┘   └──────────────┘                         │
                                                               │
   ┌──────────────────────────────────────────────────────────┘
   ▼
   shared base: tome-core (errors, types) + tome-config (paths, defaults)
```

**Rules of the dep graph:**
- The UI talks to `tome-services` and nothing else.
- `tome-services` may depend on every other crate.
- Domain crates (`tome-dump`, `tome-storage`, `tome-api`, `tome-search`, `tome-wikitext`, `tome-modules`) depend only on `tome-core`, `tome-config`, and where strictly necessary on each other (documented per crate). Saved-revision archival lives in `tome-storage`'s `archive` submodule alongside the article store.
- `tome-core` and `tome-config` depend on nothing in this workspace except possibly each other.
- No cycles. Cargo enforces this at compile time.

## Layer responsibilities

### Dump access layer (`tome-dump`)
- Stream-parses the multistream bz2 index to populate the offset map.
- Given `(offset, length)`, decompresses one stream and parses the `<page>` records inside it.
- Verifies the dump file's SHA1 against the official checksum.
- **Never** loads the entire dump into memory.

### Storage layer (`tome-storage`)
- Owns the SQLite schema: articles, tier assignments, offset index, last-access timestamps, module membership.
- Owns the per-article zstd compression for the Warm tier.
- Implements LRU promotion/demotion between Hot ↔ Warm and demotion to Cold.
- Looks up article content by tier; for Cold, delegates to `tome-dump`.
- Pinned articles bypass automatic transitions.

### API client (`tome-api`)
- The **sole gatekeeper** for outbound network traffic. Other crates request data through this; they never call `reqwest` directly.
- Enforces 10 req/s ceiling, exponential backoff, `Retry-After` honoring, circuit breaker, kill switch.
- Wraps batch endpoints (`titles=A|B|C`) for any caller asking for multiple items.
- Caches API responses on disk; never refetches an immutable revision.
- Maintains a 1000-entry circular log buffer for the debug view.
- `Clock` and `HttpTransport` are abstracted as traits so the gatekeeper logic is fully testable without a live network.

### Search engine (`tome-search`)
- Tantivy index built incrementally during ingestion. Streaming pipeline: dump stream → text extraction → analyzer → segment writer.
- Query layer: BM25 + stemming + WordNet expansion + redirect-aware matching + link-graph weighting.
- Filters by module, tier, and date are evaluated at query time.

### Wiki parser (`tome-wikitext`)
- Two render paths: cached Parsoid HTML from the MediaWiki Core REST endpoint (preferred when online), and a local Rust renderer over the `parse-wiki-text-2` AST (offline fallback, structural-only).
- Internal links are resolved via the storage layer's offset index — present articles get live links, missing ones a visual marker.
- Output HTML is cached so each (article, revision) is rendered at most once per render path.

### Module manager (`tome-modules`)
- Module definitions: name, list of categories with depths, list of explicit titles, default tier.
- Category resolution traverses Wikipedia's category tree via the API client (batched).
- Install: resolve titles, set tier, queue Parsoid HTML fetches.
- Uninstall: move articles to Cold or Evicted per user choice.
- Import/export to a portable file format (TOML).

### Revision archive (`tome-storage::archive`)
- Separate SQLite database (alongside the article store, in the same crate) for permanently-saved revisions with optional user notes.
- Independent of the tier system; saved revisions are full content and survive dump replacement.
- Diffs computed via `action=compare` through the API client.
- Local FTS5 index for searching within saved revisions.

### Services (`tome-services`)
- Composes lower-layer operations into user-facing flows: ingest dump, install module, search, render article, save revision, refresh from API.
- Exposed to the UI as Tauri command handlers.
- Holds no persistent state of its own — pure orchestration.

## Data flows

### Cold-tier article read (online)

```
UI: requestArticle("Photon")
  → services.read_article(title)
    → storage.lookup(title) → tier=Cold, offset=12_345_678, len=98_765
    → wikitext.render(title, revid)
      → cache.lookup(title, revid) → miss
      → api.fetch_html(title)         (gatekeeper enforces rate / backoff / cache)
      → cache.store(title, revid, html)
    ← html
  ← html
UI: render html
storage.touch(title)  → access count bumped, may promote to Warm
```

### Cold-tier article read (offline)

```
UI: requestArticle("Photon")
  → services.read_article(title)
    → storage.lookup(title) → tier=Cold, offset=...
    → wikitext.render(title, revid)
      → cache.lookup → miss
      → api.fetch_html → KillSwitch / network error
      → fallback: dump.read_stream(offset, len)
        → bz2 decode → XML parse → wikitext bytes
      → parse_wiki_text_2.parse(wikitext) → AST
      → ast_to_html(AST) → html (lossy on templates)
    ← html
  ← html
UI: render html with "offline render" indicator
```

### Search index build

```
ingest_dump(path):
  for each (offset, length) in index_map:
    bytes = decompress_stream(offset, length)
    for each <page> in bytes:
      title, text = parse_page(<page>)
      tantivy_writer.add_document({title, text})
      if tantivy_writer.batch_size > THRESHOLD:
        tantivy_writer.commit()
    bytes is dropped
  tantivy_writer.commit()
```

Memory ceiling: one decompressed stream (~100KB–1MB) + one Tantivy commit batch.

### Module install

```
services.install_module(spec):
  titles = modules.resolve_categories(spec.categories)  (batched via api.list_category_members)
  for batch of 50 titles:
    storage.set_tier(batch, spec.default_tier)
    api.fetch_html_batch(batch) → cache
  search.refresh_module_filter(spec.id)
```

## Runtime conventions

- All paths derived at runtime from `dirs::data_dir().join("Tome")`. No hardcoded user paths in source.
- `User-Agent` loaded from `tome-config::Config::user_agent`; default is the standard `Tome/1.0 (+https://github.com/HesNotTheGuy/tome)` string.
- Logging uses a redaction layer that strips user-entered text (notes, full article bodies) before emit so log files stay safe to share for debugging.

## Testing strategy

| Layer | Style | Notes |
|---|---|---|
| `tome-dump` | Unit + integration with a tiny fixture multistream | Generated bz2 fixture with 2-3 streams, no real Wikipedia |
| `tome-storage` | Unit, all tier transitions covered | tempfile-backed SQLite |
| `tome-api` | Unit with `wiremock` + manual `Clock` | Exhaustive: rate limit, backoff, Retry-After, circuit breaker, batching, caching, kill switch |
| `tome-search` | Unit + integration with small fixture corpus | ~50 articles |
| `tome-wikitext` | Snapshot tests against ~50 hand-picked articles | Featured / stub / disambig / list / heavy-template variants |
| `tome-modules` | Unit with mocked API client | Category tree resolution, install/uninstall flows |
| `tome-storage` (archive submodule) | Unit | tempfile-backed SQLite |
| `tome-services` | Integration: dump → install → search → save → refresh | The flow listed in the spec |
