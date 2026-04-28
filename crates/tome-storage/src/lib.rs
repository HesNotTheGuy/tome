//! Tiered article storage.
//!
//! Owns the per-article placement across four tiers:
//!
//! - **Hot**: plain wikitext in SQLite for instant read.
//! - **Warm**: zstd-compressed bytes, decompressed on access.
//! - **Cold**: not stored; metadata records the dump offset/length so the
//!   caller can resolve content via the [`tome-dump`](../tome_dump/index.html)
//!   crate.
//! - **Evicted**: explicitly excluded; a sentinel value telling callers to
//!   confirm before fetching.
//!
//! All schema migrations and the per-article compression policy live here so
//! the rest of the app does not need to know about them.

pub mod article;
pub mod compression;
pub mod geotag;
pub mod schema;
pub mod store;

pub use article::{ArticleContent, ArticleMetadata, ArticleRecord};
pub use geotag::Geotag;
pub use store::{ArticleStore, SqliteArticleStore};
