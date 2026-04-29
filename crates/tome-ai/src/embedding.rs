//! Semantic search via dense embeddings.
//!
//! # What this does
//!
//! Given a string of text, produce a fixed-size dense vector that represents
//! its semantic content. Two texts whose vectors have a small cosine angle
//! are roughly "about the same thing," even when they share no literal
//! tokens. Used for two things:
//!
//! 1. **Article ingest**: embed every article's title + lede once; store the
//!    vector keyed by `page_id`.
//! 2. **Query time**: embed the user's search query; cosine-rank the stored
//!    vectors; return top-K by similarity.
//!
//! # Model choice
//!
//! Default is `BGE-small-en-v1.5` via [`fastembed`]:
//! - 384 dimensions (1.5 KB per article in f32, ~370 MB for simplewiki).
//! - ~33 MB on-disk, auto-downloaded on first use.
//! - English-only. Multilingual variants exist; we'll add a config knob if
//!   anyone asks.
//!
//! # Build / runtime cost
//!
//! `fastembed` is gated behind the `semantic-search` feature because it
//! pulls in ONNX Runtime — a ~100 MB native dependency that adds ~10 min
//! to a cold compile. The Tauri shell enables the feature; cargo-test runs
//! and library consumers that don't want AI can stay on the stock build.

use std::path::PathBuf;

use tome_core::{Result, TomeError};

/// A text embedder producing fixed-dimension dense vectors.
pub trait Embedder: Send + Sync {
    /// Vector dimensionality. Stable for the lifetime of the embedder.
    fn dim(&self) -> usize;

    /// Embed a single string. Convenience wrapper around `embed_batch`.
    fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut out = self.embed_batch(&[text.to_string()])?;
        out.pop()
            .ok_or_else(|| TomeError::Other("embedder returned no vectors".into()))
    }

    /// Embed many strings. Implementations should batch under the hood
    /// since transformer inference is dominated by per-call overhead.
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Cosine similarity between two same-dimension vectors. Inputs are not
/// required to be normalized; we compute it from scratch.
///
/// Returns a value in `[-1.0, 1.0]`. Higher is more similar. Returns `0.0`
/// when either vector is the zero vector (all components zero) — those
/// shouldn't occur from a real embedder but we don't want to NaN out the
/// caller's ranking.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "cosine of mismatched dims");
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[cfg(feature = "semantic-search")]
mod fastembed_impl {
    use super::*;
    use std::sync::Mutex;

    use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

    /// Default embedder backed by [`fastembed`] + BGE-small-en-v1.5.
    ///
    /// First instantiation downloads the model (~33 MB) into `cache_dir`.
    /// The download is synchronous and blocking — call `new` from a worker
    /// thread, not the UI thread. Subsequent runs read from the cached
    /// files instantly.
    pub struct DefaultEmbedder {
        // fastembed's TextEmbedding takes &mut self for predict, so we wrap
        // it in a Mutex to keep the trait API ergonomic (&self).
        inner: Mutex<TextEmbedding>,
        dim: usize,
    }

    impl DefaultEmbedder {
        pub fn new(cache_dir: PathBuf) -> Result<Self> {
            std::fs::create_dir_all(&cache_dir)
                .map_err(|e| TomeError::Other(format!("create AI cache dir: {e}")))?;

            let opts = InitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(false);

            let model = TextEmbedding::try_new(opts)
                .map_err(|e| TomeError::Other(format!("init BGE-small embedder: {e}")))?;

            Ok(Self {
                inner: Mutex::new(model),
                dim: 384,
            })
        }
    }

    impl Embedder for DefaultEmbedder {
        fn dim(&self) -> usize {
            self.dim
        }

        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            let mut model = self
                .inner
                .lock()
                .map_err(|e| TomeError::Other(format!("embedder mutex poisoned: {e}")))?;
            // fastembed accepts AsRef<str>; cloning Vec<String> would be
            // wasteful — pass slice references via collect.
            let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            let result = model
                .embed(refs, None)
                .map_err(|e| TomeError::Other(format!("embed batch: {e}")))?;
            Ok(result)
        }
    }
}

#[cfg(feature = "semantic-search")]
pub use fastembed_impl::DefaultEmbedder;

#[cfg(not(feature = "semantic-search"))]
mod stub_impl {
    use super::*;

    /// Stub used when the `semantic-search` feature is disabled. Every
    /// method errors out with a clear "feature not compiled" message so
    /// callers know to enable the feature flag rather than silently
    /// returning empty vectors.
    #[derive(Debug)]
    pub struct DefaultEmbedder;

    impl DefaultEmbedder {
        pub fn new(_cache_dir: PathBuf) -> Result<Self> {
            Err(TomeError::Other(
                "semantic search disabled: rebuild with --features semantic-search".into(),
            ))
        }
    }

    impl Embedder for DefaultEmbedder {
        fn dim(&self) -> usize {
            0
        }
        fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Err(TomeError::Other(
                "semantic search disabled: rebuild with --features semantic-search".into(),
            ))
        }
    }
}

#[cfg(not(feature = "semantic-search"))]
pub use stub_impl::DefaultEmbedder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_of_identical_vectors_is_one() {
        let a = vec![0.1, 0.2, 0.3];
        let b = vec![0.1, 0.2, 0.3];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_opposite_vectors_is_negative_one() {
        let a = vec![0.1, 0.2, 0.3];
        let b = vec![-0.1, -0.2, -0.3];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_orthogonal_vectors_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_zero_vector_is_zero_not_nan() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_handles_unnormalized_inputs() {
        let a = vec![2.0, 0.0];
        let b = vec![5.0, 0.0]; // same direction, larger magnitude
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[cfg(not(feature = "semantic-search"))]
    #[test]
    fn stub_returns_clear_error_when_feature_disabled() {
        let r = DefaultEmbedder::new(PathBuf::from("/tmp/never"));
        assert!(r.is_err());
        let msg = format!("{}", r.unwrap_err());
        assert!(msg.contains("semantic-search"));
    }
}
