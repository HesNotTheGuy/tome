//! Local LLM chat for retrieval-augmented "Ask Tome" answers.
//!
//! # Architecture
//!
//! - The user types a question in the Reader's "Ask Tome" panel.
//! - The Tome facade composes a retrieval set (lexical + semantic) of K
//!   articles, packs the most relevant snippets into a prompt with stable
//!   `[A1]`, `[A2]` … citation tokens.
//! - A [`ChatEngine`] streams tokens back; the UI renders prose with the
//!   citation tokens replaced by clickable references that open the source
//!   article in the Reader.
//! - **GBNF grammar** constrains the model's output to JSON of shape
//!   `{"answer": "…", "citations": [1, 3]}` so we can mechanically verify
//!   that every citation refers to a real chunk in the retrieval set.
//!   Answers without citations are rejected — the anti-hallucination guard.
//!
//! # Backend choice
//!
//! Default backend is [`llama-cpp-2`] (Rust bindings to llama.cpp).
//! Recommended in `docs/research/local-llm-landscape.md` for production
//! desktop use because it tracks upstream quantization / model formats
//! within days of release. Build cost is real: ~5-10 min cold compile,
//! requires a C++ toolchain. We gate it behind the `chat` feature so a
//! stock build doesn't pay either cost.
//!
//! Default model: **Phi-4-mini-instruct** at Q4_K_M (~2.3 GB). MIT licensed,
//! follows citation-style prompts well per the research notes. The model
//! identifier is configurable so power users can swap in larger or
//! differently-licensed weights.
//!
//! # Streaming
//!
//! [`ChatEngine::stream`] returns an iterator of token deltas plus a final
//! result containing the parsed citation list. The UI renders deltas as
//! they arrive; cancellation is by dropping the iterator.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tome_core::{Result, TomeError};

/// One article snippet packed into the prompt as retrieval context.
///
/// `id` is the in-prompt citation index (1-based; matches the literal
/// `[A1]`, `[A2]` … tokens we injected into the system prompt). `page_id`
/// is the underlying Wikipedia article id, retrievable from the index for
/// click-to-open behavior in the Reader.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatContext {
    pub id: u32,
    pub page_id: u64,
    pub title: String,
    pub snippet: String,
}

/// One streamed chunk of a model response. `Token` arrives as the model
/// generates; `Done` is emitted exactly once when generation completes
/// successfully and carries the parsed citation list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatChunk {
    Token(String),
    Done(ChatAnswer),
}

/// Final answer parsed out of the model's GBNF-constrained JSON output.
///
/// `citations` indexes into the [`ChatContext`] list that was passed in
/// — never raw page ids — so a model that hallucinates a citation index
/// outside the bounds of the retrieval set is rejected at parse time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAnswer {
    pub answer: String,
    pub citations: Vec<u32>,
}

/// Configuration for instantiating a [`ChatEngine`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Where to cache downloaded model weights. Created on init.
    pub cache_dir: PathBuf,
    /// HuggingFace model identifier, e.g. `microsoft/Phi-4-mini-instruct`.
    pub model_repo: String,
    /// Specific GGUF file inside the repo to use.
    pub model_file: String,
    /// Sampling temperature. 0.0 = greedy, 0.7 typical, 1.0 high-variance.
    pub temperature: f32,
    /// Hard cap on output tokens. Keeps a runaway generation bounded.
    pub max_tokens: u32,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            cache_dir: PathBuf::from("./ai-cache"),
            model_repo: "microsoft/Phi-4-mini-instruct-GGUF".into(),
            model_file: "phi-4-mini-instruct-Q4_K_M.gguf".into(),
            temperature: 0.7,
            max_tokens: 512,
        }
    }
}

/// A local-only chat engine that generates citation-grounded answers.
pub trait ChatEngine: Send + Sync {
    /// Synchronously produce a complete answer. Implementations may
    /// internally drive a streaming generation and aggregate; for live
    /// UI streaming, see [`ChatEngine::stream`].
    fn answer(&self, question: &str, context: &[ChatContext]) -> Result<ChatAnswer>;

    /// Stream tokens as they're generated. The returned iterator yields
    /// [`ChatChunk::Token`] for each emitted token and exactly one
    /// [`ChatChunk::Done`] at the end. Drop the iterator to cancel.
    fn stream(
        &self,
        question: &str,
        context: &[ChatContext],
    ) -> Box<dyn Iterator<Item = Result<ChatChunk>> + Send + '_>;
}

#[cfg(feature = "chat")]
mod llama_impl {
    use super::*;

    /// Default chat engine backed by `llama-cpp-2`.
    ///
    /// Lazy-loads the GGUF model on first call. Subsequent calls reuse
    /// the loaded model. Wraps the inner state in `Mutex` because
    /// llama.cpp's context object is not `Sync`.
    ///
    /// **TODO (next commit):** wire actual llama-cpp-2 init + generate
    /// loop + GBNF grammar. This stub returns an "implementation pending"
    /// error so the rest of the codebase can take a dependency on the
    /// trait without waiting for the full backend.
    pub struct LlamaChatEngine {
        _config: ChatConfig,
    }

    impl LlamaChatEngine {
        pub fn new(config: ChatConfig) -> Result<Self> {
            std::fs::create_dir_all(&config.cache_dir)
                .map_err(|e| TomeError::Other(format!("create chat cache dir: {e}")))?;
            Ok(Self { _config: config })
        }
    }

    impl ChatEngine for LlamaChatEngine {
        fn answer(&self, _question: &str, _context: &[ChatContext]) -> Result<ChatAnswer> {
            Err(TomeError::Other(
                "chat backend not yet wired to llama-cpp-2; tracking in next commit".into(),
            ))
        }

        fn stream(
            &self,
            _question: &str,
            _context: &[ChatContext],
        ) -> Box<dyn Iterator<Item = Result<ChatChunk>> + Send + '_> {
            Box::new(std::iter::once(Err(TomeError::Other(
                "chat backend not yet wired to llama-cpp-2; tracking in next commit".into(),
            ))))
        }
    }
}

#[cfg(feature = "chat")]
pub use llama_impl::LlamaChatEngine;

#[cfg(not(feature = "chat"))]
mod stub_impl {
    use super::*;

    /// Stub when the `chat` feature is disabled. Every method returns a
    /// clear "feature not compiled" error so the UI can render a "Chat
    /// not available" state rather than failing mysteriously.
    #[derive(Debug)]
    pub struct LlamaChatEngine;

    impl LlamaChatEngine {
        pub fn new(_config: ChatConfig) -> Result<Self> {
            Err(TomeError::Other(
                "chat disabled: rebuild with --features chat".into(),
            ))
        }
    }

    impl ChatEngine for LlamaChatEngine {
        fn answer(&self, _question: &str, _context: &[ChatContext]) -> Result<ChatAnswer> {
            Err(TomeError::Other(
                "chat disabled: rebuild with --features chat".into(),
            ))
        }
        fn stream(
            &self,
            _question: &str,
            _context: &[ChatContext],
        ) -> Box<dyn Iterator<Item = Result<ChatChunk>> + Send + '_> {
            Box::new(std::iter::once(Err(TomeError::Other(
                "chat disabled: rebuild with --features chat".into(),
            ))))
        }
    }
}

#[cfg(not(feature = "chat"))]
pub use stub_impl::LlamaChatEngine;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let c = ChatConfig::default();
        assert!(!c.model_repo.is_empty());
        assert!(c.model_file.ends_with(".gguf"));
        assert!(c.temperature >= 0.0 && c.temperature <= 2.0);
        assert!(c.max_tokens > 0);
    }

    #[test]
    fn chat_context_serializes_round_trip() {
        let ctx = ChatContext {
            id: 1,
            page_id: 23535,
            title: "Photon".into(),
            snippet: "A photon is an elementary particle.".into(),
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let back: ChatContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, 1);
        assert_eq!(back.page_id, 23535);
    }

    #[test]
    fn chat_answer_serializes_round_trip() {
        let a = ChatAnswer {
            answer: "Photons are quanta of light.".into(),
            citations: vec![1, 3],
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: ChatAnswer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.answer, "Photons are quanta of light.");
        assert_eq!(back.citations, vec![1, 3]);
    }

    #[cfg(not(feature = "chat"))]
    #[test]
    fn stub_returns_clear_error_when_feature_disabled() {
        let r = LlamaChatEngine::new(ChatConfig::default());
        assert!(r.is_err());
        let msg = format!("{}", r.unwrap_err());
        assert!(msg.contains("chat") || msg.contains("disabled"));
    }
}
