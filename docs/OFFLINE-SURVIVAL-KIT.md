# The Offline Survival Kit

Everything Tome needs lives in a handful of files you download once. **You must gather
them while you still have internet** — afterward there is no way to fetch what's missing.
This page is the complete shopping list and the setup order, written so you can print it,
check things off, and walk away with a laptop that works with no network at all.

A USB drive (64 GB or larger for full English Wikipedia, 8 GB is plenty for Simple
English) holds the whole kit with room to spare.

## The checklist

### Required — Tome won't work without these

| Done | File | Where to get it | English (full) | Simple English |
|------|------|-----------------|----------------|----------------|
| ☐ | The Tome installer for your OS | <https://github.com/HesNotTheGuy/tome/releases> | ~80 MB | ~80 MB |
| ☐ | Article dump — `*-pages-articles-multistream.xml.bz2` | <https://dumps.wikimedia.org/enwiki/latest/> or <https://dumps.wikimedia.org/simplewiki/latest/> | ~24 GB | ~300 MB |
| ☐ | Dump index — `*-multistream-index.txt.bz2` | Same page as the dump | ~250 MB | ~3 MB |

> **The dump and its index must come from the same dump date.** Download them in the same
> sitting, from the same page. A mismatched pair produces "page not found in stream"
> errors because the article positions won't line up.

Not sure which wiki to pick? Simple English is a real, useful encyclopedia at 1% of the
size — a good first run, and you can ingest the full English dump later.

### Optional — each one unlocks a feature

| Done | File | Where to get it | English (full) | Simple English | What it unlocks |
|------|------|-----------------|----------------|----------------|-----------------|
| ☐ | `*-categorylinks.sql.gz` | Same dumps page | ~2.4 GB | ~28 MB | Browse pane (explore by category) + related articles in the Reader |
| ☐ | `*-geo_tags.sql.gz` | Same dumps page | ~50 MB | ~1 MB | Map pins + coordinates on geographic articles |
| ☐ | `*-redirect.sql.gz` | Same dumps page | ~150 MB | ~1 MB | Typing "USA" lands on "United States" |
| ☐ | A basemap — any `.pmtiles` file | <https://maps.protomaps.com/builds/> | your choice | your choice | The actual map behind the Map pane's pins. Whole planet is ~120 GB; a single country or region is far smaller |
| ☐ | Chat model — `phi-4-mini-instruct-Q4_K_M.gguf` | <https://huggingface.co/microsoft/Phi-4-mini-instruct-GGUF> | ~2.3 GB | ~2.3 GB | Ask Tome (offline AI that answers questions with citations) |
| ☐ | Embedding model (see "Preparing the AI features" below — it isn't a normal download) | Fetched by Tome itself | ~33 MB | ~33 MB | Search by meaning, not just by title |

Skipping an optional file never breaks anything else — the feature it unlocks just
stays dormant until you ingest it. You can add any of them later (from a newer dump
date, even; see "Keeping your data safe").

## Set-up order

Do these in order, on the machine that will go offline. Total active time is a few
minutes; the long steps run unattended.

1. **Install Tome** and launch it. The first-run wizard appears.
2. **Point the wizard at the dump and the index** you downloaded. Ingesting the index
   takes about 1–5 minutes for Simple English, longer for full English. **Don't close
   the app while it runs.** When it finishes, reading and title search work — the core
   of Tome is now offline-ready.
3. **Settings → Categorylinks ingestion** — pick your `*-categorylinks.sql.gz`. This is
   the slow one: budget a few minutes for Simple English, **30+ minutes for full
   English. Leave the app open** until it reports done.
4. **Settings → Geotag ingestion** — pick your `*-geo_tags.sql.gz`. A few minutes.
5. **Settings → Redirects ingestion** — pick your `*-redirect.sql.gz`. A few minutes.
6. **Settings → Offline map source** — point it at your `.pmtiles` file. Instant.
7. **Prepare the AI features** (next section) — this is the step people forget, and
   half of it *requires* being online.
8. **Test everything while you still have internet** so you can re-download anything
   that's broken: open an article, search for one, open Browse, open the Map, ask
   Ask Tome a question, try a "by meaning" search.
9. **Settings → Network → switch "Offline mode" on.** This is the last step. It blocks
   all outbound traffic and makes every article resolve from your local data instantly.
   Tome is now fully self-contained.

## Preparing the AI features

Both AI features need model files. The chat model is a normal download; the small
search model is fetched by Tome itself and needs internet exactly once.

### Ask Tome (the chat model)

Two ways to get it — pick one:

- **While online:** Settings → Ask Tome → **Download**. One click, ~2.3 GB.
- **Side-load (works fully offline):** download `phi-4-mini-instruct-Q4_K_M.gguf` from
  <https://huggingface.co/microsoft/Phi-4-mini-instruct-GGUF> on any connected machine,
  copy it to the offline machine (USB drive is fine), then in **Settings → Ask Tome**
  use the path picker to point Tome at the file. No internet needed on the target
  machine.

### Search by meaning (the embedding model)

The first time you click **Embed articles** in Settings → Semantic search, Tome
auto-downloads a small model (~33 MB, BGE-small-en-v1.5) from the internet. There is no
download button for this one — clicking that button while online IS the download.

So, while you still have internet: click **Embed articles** once and let it start.
The model is now cached and the feature works offline forever after. (The embedding run
itself is resumable — interrupting it and re-running later picks up where it left off.)

If your target machine never gets online at all, use the folder-copy trick:

1. On any machine that has run Tome's "Embed articles" step while online, find Tome's
   data folder:
   - **Windows:** `%APPDATA%\Tome` (paste that into the File Explorer address bar)
   - **macOS:** `~/Library/Application Support/Tome` (in Finder: Go → Go to Folder…)
   - **Linux:** `~/.local/share/Tome`
2. Inside it, copy the entire `ai/models` folder onto your USB drive.
3. On the offline machine, copy it into the same place — the `ai/models` folder inside
   that machine's Tome data folder (create the `ai` folder if it doesn't exist).

That's it. Tome finds the cached model and never asks the network for it.

## Two-machine workflow

If the offline machine has no internet (or only censored/untrusted internet), prepare
everything on a connected machine:

1. On the connected machine, download every checked item from the checklist onto the
   USB drive.
2. For the embedding model, install Tome on the connected machine too, click
   **Embed articles** once, and copy its `ai/models` folder to the USB drive (steps
   above).
3. Carry the drive to the offline machine, install Tome, and follow the set-up order.
   Use the side-load path for the chat model and the folder-copy trick for the
   embedding model.

You can run Tome straight off the USB drive's files — pointing Tome at dumps, models,
and `.pmtiles` on the drive works fine. Copying them onto the internal drive first makes
article reads faster, and frees the USB drive to live in a drawer as your backup.

## Troubleshooting offline

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| "page not found in stream" when opening articles | Dump and index are from different dump dates | Re-download both from the same dated folder and re-run the index ingest |
| Articles load, but slowly, or only after a pause | Offline mode isn't on, so Tome is still trying the network first | Settings → Network → switch Offline mode on |
| Search box finds nothing | The index was never ingested | Run the first-run wizard (Settings → "Run setup again") or Settings → Dump ingestion |
| Browse pane is missing or empty | Categorylinks not ingested | Settings → Categorylinks ingestion (the 30+ minute one) |
| Map shows pins on a blank background | No `.pmtiles` basemap configured | Settings → Offline map source → point at your `.pmtiles` file |
| Ask Tome button missing, or errors when used | Chat model file not present | Side-load it: Settings → Ask Tome → path picker → your `.gguf` file |
| "By meaning" search errors | Embedding model was never cached (needs internet once) | Copy `ai/models` from a machine that ran Embed articles online — see "Preparing the AI features" |
| Reader works but coordinates never appear | geo_tags not ingested | Settings → Geotag ingestion |
| "USA", "UK" etc. find nothing | Redirects not ingested | Settings → Redirects ingestion |

## Keeping your data safe

- **Export your bookmarks periodically.** The Bookmarks pane has Export/Import — backups
  are versioned JSON files that future versions of Tome will always be able to read.
  Export to the USB drive so a laptop failure doesn't take your reading history's
  greatest hits with it.
- **Tome never modifies your files.** Dumps, indexes, models, and `.pmtiles` are opened
  strictly read-only. The copy on your USB drive stays byte-for-byte pristine no matter
  how much you use it — it's a permanent backup by definition.
- **Re-ingesting is always safe.** If you later get your hands on a newer dump
  (and its matching index!), point Tome at the new pair and re-run the ingests. Nothing
  needs to be uninstalled or reset first.

## FAQ

**Can I update articles while offline?**
No. Your dump is a snapshot of Wikipedia on its dump date. It doesn't change until you
get online (or get a newer dump on a USB drive) and re-ingest.

**Can I add a single missing article?**
No — articles come from the dump, whole. If something's missing, the fix is a bigger or
newer dump: for example moving from Simple English to full English, then re-running the
ingest.

**Does this work on a phone or tablet?**
No. Tome is desktop-only: Windows, macOS, and Linux.

**Do I need the internet for anything after setup?**
No. That's the point. Every feature in this guide — reading, search, Browse, the Map,
Ask Tome, search by meaning — runs entirely from the files on your disk.
