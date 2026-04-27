//! Full-text search.
//!
//! Tantivy-backed index built once during ingestion by streaming each
//! decompressed bz2 stream, extracting article plaintext, batch-writing into
//! the index, and dropping the buffer. Memory ceiling is one stream + one
//! batch; the full corpus is never resident.
//!
//! Query layer:
//! - BM25 ranking
//! - Stemming + WordNet-based query expansion (bundled offline)
//! - Redirect-aware matching
//! - Link-graph importance weighting
//! - Filters: module, tier, date
//!
//! Implementation ships in step 5 of the build order.
