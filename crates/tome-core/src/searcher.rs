//! Cross-crate search abstraction.
//!
//! Implemented by both the lexical (BM25) and semantic (vector) backends.
//! Lets `tome-services` compose multiple providers into a hybrid ranker
//! without either backend depending on the other.

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::tier::Tier;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub page_id: u64,
    pub title: String,
    pub tier: Tier,
    pub score: f32,
}

/// A search backend. Stateless from the caller's perspective — the index it
/// queries is owned internally.
pub trait Searcher: Send + Sync {
    /// Run a query and return the top-`limit` hits ordered by score
    /// descending. Empty `tier_filter` matches all tiers.
    fn search(&self, query: &str, limit: usize, tier_filter: &[Tier]) -> Result<Vec<SearchHit>>;

    /// A short identifier for telemetry / debugging (e.g. "bm25", "vector").
    fn name(&self) -> &str;
}
