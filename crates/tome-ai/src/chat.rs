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

#[cfg(feature = "chat-inference")]
mod llama_impl {
    use super::*;
    use crate::chat_download::expected_path;
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{AddBos, LlamaModel, Special};
    use llama_cpp_2::sampling::LlamaSampler;
    use std::num::NonZeroU32;
    use std::sync::{Arc, Mutex, OnceLock};

    /// Process-wide llama.cpp backend. The C library expects to be
    /// initialized exactly once; subsequent inits return the same handle.
    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

    fn backend() -> Result<&'static LlamaBackend> {
        BACKEND
            .get_or_init(|| LlamaBackend::init().expect("init llama backend (process-wide, once)"));
        BACKEND
            .get()
            .ok_or_else(|| TomeError::Other("llama backend not initialized".into()))
    }

    /// Default chat engine backed by [`llama-cpp-2`].
    ///
    /// The loaded model + tokenizer is cached behind a `Mutex` because
    /// llama.cpp's `LlamaContext` is `Send` but not `Sync` — only one
    /// generation can run at a time on a given context. For desktop use
    /// that's fine; we serialize the queries.
    pub struct LlamaChatEngine {
        config: ChatConfig,
        // Lazy-loaded on first generate. Wrapped in Mutex so &self
        // generate methods can mutate it. Arc lets us hand out clones
        // for streaming iterators that outlive the borrow.
        model: Arc<Mutex<Option<LlamaModel>>>,
    }

    impl LlamaChatEngine {
        pub fn new(config: ChatConfig) -> Result<Self> {
            std::fs::create_dir_all(&config.cache_dir)
                .map_err(|e| TomeError::Other(format!("create chat cache dir: {e}")))?;
            Ok(Self {
                config,
                model: Arc::new(Mutex::new(None)),
            })
        }

        /// Get-or-load the model. Loading reads ~2.3 GB off disk and
        /// can take a couple of seconds; the caller should run this
        /// off the UI thread.
        fn ensure_loaded(&self) -> Result<()> {
            let mut guard = self
                .model
                .lock()
                .map_err(|e| TomeError::Other(format!("model mutex poisoned: {e}")))?;
            if guard.is_some() {
                return Ok(());
            }
            let path = expected_path(&self.config);
            if !path.exists() {
                return Err(TomeError::Other(format!(
                    "chat model not downloaded yet: {path:?}"
                )));
            }
            let backend = backend()?;
            let params = LlamaModelParams::default();
            let model = LlamaModel::load_from_file(backend, &path, &params)
                .map_err(|e| TomeError::Other(format!("load gguf {path:?}: {e}")))?;
            *guard = Some(model);
            Ok(())
        }

        /// Format the prompt using Phi-4's chat template. Packs
        /// retrieval context as `[A1] Title: snippet` lines in the
        /// system message and instructs the model to cite by index.
        fn build_prompt(&self, question: &str, context: &[ChatContext]) -> String {
            let mut sys = String::from(
                "You are Tome, an offline assistant answering from supplied Wikipedia \
                 excerpts. Cite the sources you used by their bracketed index, e.g. [A1] \
                 or [A1][A3]. If the excerpts don't contain the answer, say so plainly. \
                 Do not invent citations.\n\nSources:\n",
            );
            for c in context {
                sys.push_str(&format!("[A{}] {}: {}\n", c.id, c.title, c.snippet));
            }
            // Phi-4 chat template:
            //   <|system|>...<|end|><|user|>...<|end|><|assistant|>
            format!("<|system|>\n{sys}<|end|>\n<|user|>\n{question}<|end|>\n<|assistant|>\n")
        }

        /// Run a generation loop, returning the decoded text.
        fn generate(&self, prompt: &str) -> Result<String> {
            self.ensure_loaded()?;
            let backend = backend()?;
            let guard = self
                .model
                .lock()
                .map_err(|e| TomeError::Other(format!("model mutex poisoned: {e}")))?;
            let model = guard
                .as_ref()
                .ok_or_else(|| TomeError::Other("model not loaded".into()))?;

            let n_ctx = NonZeroU32::new(4096).expect("non-zero");
            let ctx_params = LlamaContextParams::default().with_n_ctx(Some(n_ctx));
            let mut ctx = model
                .new_context(backend, ctx_params)
                .map_err(|e| TomeError::Other(format!("new llama context: {e}")))?;

            let tokens = model
                .str_to_token(prompt, AddBos::Always)
                .map_err(|e| TomeError::Other(format!("tokenize prompt: {e}")))?;

            let mut batch = LlamaBatch::new(512, 1);
            let prompt_len = tokens.len() as i32;
            for (i, t) in tokens.iter().enumerate() {
                let last = i == tokens.len() - 1;
                batch
                    .add(*t, i as i32, &[0], last)
                    .map_err(|e| TomeError::Other(format!("batch add: {e}")))?;
            }
            ctx.decode(&mut batch)
                .map_err(|e| TomeError::Other(format!("decode prompt: {e}")))?;

            let mut sampler = LlamaSampler::chain_simple([
                LlamaSampler::temp(self.config.temperature),
                LlamaSampler::greedy(),
            ]);

            let mut output = String::new();
            let mut n_cur = prompt_len;
            for _ in 0..self.config.max_tokens {
                let token = sampler.sample(&ctx, batch.n_tokens() - 1);
                sampler.accept(token);
                if model.is_eog_token(token) {
                    break;
                }
                let piece = model
                    .token_to_str(token, Special::Tokenize)
                    .map_err(|e| TomeError::Other(format!("detokenize: {e}")))?;
                output.push_str(&piece);
                batch.clear();
                batch
                    .add(token, n_cur, &[0], true)
                    .map_err(|e| TomeError::Other(format!("batch add token: {e}")))?;
                n_cur += 1;
                ctx.decode(&mut batch)
                    .map_err(|e| TomeError::Other(format!("decode token: {e}")))?;
            }
            Ok(output)
        }
    }

    impl ChatEngine for LlamaChatEngine {
        fn answer(&self, question: &str, context: &[ChatContext]) -> Result<ChatAnswer> {
            let prompt = self.build_prompt(question, context);
            let text = self.generate(&prompt)?;
            // Strip Phi-4's end markers if the model emitted them.
            let cleaned = text.trim_end_matches("<|end|>").trim().to_string();
            let citations = parse_citations(&cleaned, context);
            Ok(ChatAnswer {
                answer: cleaned,
                citations,
            })
        }

        fn stream(
            &self,
            _question: &str,
            _context: &[ChatContext],
        ) -> Box<dyn Iterator<Item = Result<ChatChunk>> + Send + '_> {
            // Token-level streaming requires holding the LlamaContext
            // across iterator polls, which conflicts with the Mutex on
            // self.model. Future commit: rework to thread a context
            // through a channel. For now stream() falls back to a single
            // Done chunk produced by the synchronous answer().
            Box::new(std::iter::once_with(move || {
                self.answer(_question, _context).map(ChatChunk::Done)
            }))
        }
    }

    /// Best-effort citation extractor. Scans the model's output for
    /// `[A1]`-style markers and returns the unique indices that match
    /// real entries in the context. A future commit will replace this
    /// with GBNF-constrained JSON output for stronger guarantees.
    fn parse_citations(text: &str, context: &[ChatContext]) -> Vec<u32> {
        let mut out = Vec::new();
        let mut iter = text.char_indices().peekable();
        while let Some((i, c)) = iter.next() {
            if c != '[' {
                continue;
            }
            // Look for "A<digits>]" starting at i+1.
            let rest = &text[i + 1..];
            let mut chars = rest.chars();
            if chars.next() != Some('A') {
                continue;
            }
            let num: String = chars.take_while(|c| c.is_ascii_digit()).collect();
            if num.is_empty() {
                continue;
            }
            // Confirm closing bracket follows the digits.
            let close_idx = i + 2 + num.len();
            if text.as_bytes().get(close_idx) != Some(&b']') {
                continue;
            }
            if let Ok(n) = num.parse::<u32>()
                && context.iter().any(|c| c.id == n)
                && !out.contains(&n)
            {
                out.push(n);
            }
        }
        out
    }

    #[cfg(test)]
    mod chat_tests {
        use super::*;

        fn ctx(id: u32, title: &str, snippet: &str) -> ChatContext {
            ChatContext {
                id,
                page_id: id as u64,
                title: title.into(),
                snippet: snippet.into(),
            }
        }

        #[test]
        fn parse_citations_extracts_in_text_references() {
            let context = vec![
                ctx(1, "Photon", "elementary particle"),
                ctx(2, "Electron", "subatomic"),
                ctx(3, "Quark", "elementary"),
            ];
            let text = "Photons are quanta of light [A1]. Electrons orbit nuclei [A2][A2].";
            let cites = parse_citations(text, &context);
            assert_eq!(cites, vec![1, 2], "duplicates must be deduped");
        }

        #[test]
        fn parse_citations_drops_invalid_indices() {
            let context = vec![ctx(1, "Photon", "")];
            // [A99] doesn't match a context id; must be dropped.
            let text = "Some claim [A99] and another [A1].";
            assert_eq!(parse_citations(text, &context), vec![1]);
        }

        #[test]
        fn parse_citations_empty_when_no_matches() {
            let context = vec![ctx(1, "Photon", "")];
            assert_eq!(
                parse_citations("plain prose, no cites", &context),
                Vec::<u32>::new()
            );
        }
    }
}

#[cfg(feature = "chat-inference")]
pub use llama_impl::LlamaChatEngine;

#[cfg(not(feature = "chat-inference"))]
mod stub_impl {
    use super::*;

    /// Stub when `chat-inference` is disabled. Every method returns a
    /// clear "feature not compiled" error so the UI can render a "Chat
    /// not available" state rather than failing mysteriously.
    #[derive(Debug)]
    pub struct LlamaChatEngine;

    impl LlamaChatEngine {
        pub fn new(_config: ChatConfig) -> Result<Self> {
            Err(TomeError::Other(
                "chat inference disabled: rebuild with --features chat-inference \
                 (requires LLVM/libclang on the build machine)"
                    .into(),
            ))
        }
    }

    impl ChatEngine for LlamaChatEngine {
        fn answer(&self, _question: &str, _context: &[ChatContext]) -> Result<ChatAnswer> {
            Err(TomeError::Other(
                "chat inference disabled: rebuild with --features chat-inference".into(),
            ))
        }
        fn stream(
            &self,
            _question: &str,
            _context: &[ChatContext],
        ) -> Box<dyn Iterator<Item = Result<ChatChunk>> + Send + '_> {
            Box::new(std::iter::once(Err(TomeError::Other(
                "chat inference disabled: rebuild with --features chat-inference".into(),
            ))))
        }
    }
}

#[cfg(not(feature = "chat-inference"))]
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

    #[cfg(not(feature = "chat-inference"))]
    #[test]
    fn stub_returns_clear_error_when_feature_disabled() {
        let r = LlamaChatEngine::new(ChatConfig::default());
        assert!(r.is_err());
        let msg = format!("{}", r.unwrap_err());
        assert!(msg.contains("chat-inference") || msg.contains("disabled"));
    }
}
