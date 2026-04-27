//! Service orchestration layer.
//!
//! The UI talks to this crate and nothing else. Service methods compose the
//! lower-level crates (dump, storage, api, search, wikitext, modules,
//! archive) into the user-facing operations exposed through Tauri commands:
//! ingest a dump, install a module, search, render an article, save a
//! revision, refresh from the API, etc.
//!
//! Keeping this layer thin means the lower crates stay testable in isolation
//! and the UI never grows knowledge of storage formats or API quirks.
//!
//! Implementation expands across steps 2-13 as the lower layers come online.
