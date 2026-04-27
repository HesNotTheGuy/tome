//! Revision archive.
//!
//! Owns the user's permanently-saved historical revisions, kept entirely
//! separate from the tier system. Saved revisions are full content, accessible
//! offline forever, and searchable independently or combined with the main
//! search index.
//!
//! Diff support uses MediaWiki's `action=compare` via the API client.
//!
//! Implementation ships in step 8 of the build order.
