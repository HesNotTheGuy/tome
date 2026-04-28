//! Retrieval-augmented generation against a local LLM.
//!
//! Implementation deferred to a follow-up commit. The shape this module is
//! intended to take:
//!
//! ```text
//! 1. User asks a question via the UI's "Ask Tome" surface.
//! 2. The composed Searcher (lexical + semantic, RRF-fused) returns the
//!    top-k articles for the question.
//! 3. Article bodies are chunked and packed into the model's context
//!    window. Each chunk carries an [N] citation token referring back to
//!    its source article.
//! 4. The LLM streams an answer that interleaves prose with [N] tokens.
//! 5. The UI replaces each [N] with a clickable citation that jumps to
//!    that article in the Reader.
//!
//! Inference backend options:
//!  - llama.cpp via FFI (GGUF models, the most battle-tested path)
//!  - candle (pure Rust, simpler build, less perf)
//!  - external sidecar (spawn `llama-server` and talk over HTTP)
//!
//! Initial target model: Phi-3 mini 4-bit (~2 GB). Larger models user-
//! selectable in Settings.
//!
//! Hard rules baked in:
//! - **Always cite.** Answers without citations are dropped, even if the
//!   model produces plausible-looking text. This is the anti-hallucination
//!   guard.
//! - **No cloud fallback.** When the local model can't answer, we say so
//!   plainly. We do not silently call out to a hosted API.
//! - **Streaming UI is mandatory.** Long generations must be cancellable
//!   mid-stream.
