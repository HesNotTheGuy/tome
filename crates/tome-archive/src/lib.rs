//! Revision archive.
//!
//! Owns the user's permanently-saved historical revisions in a SQLite
//! database separate from the main article store. Saved revisions are full
//! content, accessible offline forever, and survive dump replacement. A
//! built-in FTS5 index lets the UI search across notes and content.
//!
//! Diff between two revisions is delegated to the MediaWiki API's
//! `action=compare` and lives in `tome-services`; this crate stores the
//! revisions, it does not generate diffs.

pub mod store;

pub use store::{ArchiveStore, SavedRevision, SavedRevisionMeta};
