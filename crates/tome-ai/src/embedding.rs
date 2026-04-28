//! Semantic search via dense embeddings.
//!
//! Implementation deferred to a follow-up commit. The shape this module is
//! intended to take:
//!
//! ```text
//! Embedder trait: text -> Vec<f32>     (384 or 768 dims, small CPU model)
//!     │
//!     ├─ DefaultEmbedder: fastembed-rs / BGE-small-en
//!     └─ ExternalEmbedder: ONNX runtime, candle, etc.
//!
//! VectorIndex (HNSW via usearch):
//!     - build_from_iter(iter of (page_id, text))   → expensive, hours-long
//!     - query(embedding, k) -> Vec<(page_id, score)>
//!
//! SemanticSearcher impl tome_core::Searcher:
//!     - Embeds the query with `Embedder`
//!     - Hits the `VectorIndex`
//!     - Returns the same Vec<SearchHit> shape as tome-search's lexical impl
//!
//! Hybrid ranking (lives in tome-services, not here):
//!     - Run lexical and semantic in parallel
//!     - Fuse via RRF: score = Σ 1/(k + rank_in_each), k = 60
//! ```
//!
//! Storage budget for full enwiki at 384-dim f32: ~10 GB. Cuts to ~5 GB at
//! f16 and ~2.5 GB at int8 quantization. The embedding build is resumable
//! (stored progress checkpoint) so a multi-hour run can be paused and
//! resumed without losing work.
