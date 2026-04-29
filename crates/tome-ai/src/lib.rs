//! Optional local-AI features for Tome.
//!
//! This crate is **not** wired up in any default build path. It exists as a
//! placeholder for two opt-in capabilities, each of which will land as its
//! own follow-up commit alongside the heavy ML dependencies they need:
//!
//! 1. **Semantic search** ([`embedding`]). A small sentence-transformers
//!    model produces dense vectors for article bodies; queries are embedded
//!    and matched via approximate-nearest-neighbor search (HNSW). Combined
//!    with the BM25 results from [`tome-search`](../tome_search/index.html)
//!    via Reciprocal Rank Fusion at the services layer for hybrid ranking.
//!
//! 2. **Local LLM with retrieval-augmented generation** ([`rag`]). The
//!    user asks a question; the existing search returns the top-k articles;
//!    those articles are fed to a small local LLM as context; the model
//!    answers with citations back to the source articles. Strictly opt-in,
//!    strictly local — no cloud calls, ever.
//!
//! ## Design boundaries
//!
//! - **No global on-by-default.** Every capability is gated by a Settings
//!   toggle. The default is off; first activation triggers a model
//!   download with explicit user consent and progress reporting.
//! - **No silent network egress.** Model downloads pass through the
//!   existing `tome-api` gatekeeper (rate limit, kill switch, log buffer)
//!   so users have one place to halt all outbound traffic.
//! - **Citations are mandatory in RAG output.** Every claim the LLM makes
//!   maps back to specific articles via the retrieval set. The UI renders
//!   inline citations and refuses to display answers that don't cite.
//! - **Replaceable.** Each module exposes a trait (e.g. [`embedding::Embedder`])
//!   and a default implementation. Power users can swap in a different
//!   backend (a different model, an external runtime) without changing
//!   call sites.
//!
//! ## Where the trait integration lives
//!
//! `tome-ai` does **not** import [`tome-search`](../tome_search/index.html).
//! Both crates implement [`tome_core::Searcher`] independently. The
//! services layer composes them into a [`HybridSearcher`] that runs each
//! provider and fuses rankings via RRF. This keeps `tome-search` AI-free
//! (compile time, binary size, cold start) for users who never enable
//! these features.

pub mod chat;
pub mod chat_download;
pub mod embedding;
pub mod rag;

/// Configuration knobs for the AI subsystem. Threaded through from
/// `tome-config` so the same settings file owns the source of truth.
///
/// `Default::default()` returns an everything-off config, which is the
/// shipping default — AI capabilities are strictly opt-in.
#[derive(Debug, Clone, Default)]
pub struct AiConfig {
    /// Master switch. When false, every AI surface is a no-op.
    pub enabled: bool,
    /// Subsystems can be toggled independently.
    pub semantic_search_enabled: bool,
    pub rag_enabled: bool,
}
