//! Service orchestration layer.
//!
//! The UI talks to this crate and nothing else. The [`Tome`] facade holds
//! references to every domain crate and composes their operations into the
//! flows the UI needs:
//!
//! - [`Tome::read_article`] — the Cold-tier read flow described in
//!   [`ARCHITECTURE.md`](../../../ARCHITECTURE.md). Resolves through the
//!   storage tier; if Cold, decodes from the dump and renders locally.
//! - [`Tome::search`] — wraps the Tantivy index.
//! - [`Tome::install_module`] — persists a module spec and its members.
//! - [`Tome::save_revision`] — appends to the revision archive.
//!
//! Each component is held as an `Arc` so the facade is cheap to clone and
//! safe to share across async tasks.

pub mod category_ingest;
pub mod geotag_ingest;
pub mod link_resolver;
pub mod redirect_ingest;
pub mod tome;

pub use link_resolver::StorageLinkResolver;
pub use tome::{
    ArticleResponse, ArticleSource, CategoryIngestSummary, GeotagSummary, IngestSummary,
    RedirectIngestSummary, TierCounts, Tome,
};
