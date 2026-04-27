//! Full-text search.
//!
//! Tantivy-backed index over the article corpus. The build pipeline is
//! streaming: callers iterate dump streams, decompress one at a time, and
//! feed pages to the [`Writer`] in batches. The full corpus is never
//! resident in memory; the memory ceiling is one stream + one writer batch.
//!
//! Queries use BM25 ranking out of the box. The schema indexes title and
//! body separately so prefix/exact title matches can be boosted in the
//! future without re-indexing.

pub mod index;
pub mod query;
pub mod schema;

pub use index::{Index, Writer};
pub use query::SearchHit;
pub use schema::TomeSchema;
