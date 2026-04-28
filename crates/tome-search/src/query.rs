//! Public query types.
//!
//! `SearchHit` is re-exported from `tome-core` so `tome-ai` can implement the
//! same `Searcher` trait without depending on `tome-search`.

pub use tome_core::SearchHit;
