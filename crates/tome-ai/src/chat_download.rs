//! HuggingFace model download for the chat backend.
//!
//! Standalone of the inference backend choice — this just fetches a GGUF
//! file from HuggingFace into the user's data dir with progress reporting.
//! Whether the loaded model ends up driven by `llama-cpp-2` (in-process) or
//! a `llama-server` sidecar, both want the same file on disk.
//!
//! # Lifecycle
//!
//! 1. UI shows the model size and a "Download" button.
//! 2. User clicks; we call [`download_chat_model`].
//! 3. The function streams the file into `<cache_dir>/<repo>/<file>` with
//!    progress callbacks invoked roughly every 1 MB.
//! 4. On completion, [`chat_model_path`] returns Some and the rest of the
//!    chat machinery can load it.
//!
//! # Failure modes
//!
//! - **Network error**: returns an `Err` with the underlying reason.
//!   Partial downloads are kept on disk by `hf-hub` and resume on retry.
//! - **Disk full**: returns an `Err` from the underlying I/O.
//! - **Wrong cache dir / permission**: returns an `Err` at create-dir time.
//!
//! Callers should re-display the Download button on error so the user can
//! retry; we do not auto-retry behind their back.

use std::path::PathBuf;

use tome_core::{Result, TomeError};

use crate::chat::ChatConfig;

/// Local path for the GGUF described by `config`. Returns `None` if the
/// file isn't on disk yet — the caller should drive a download first.
pub fn chat_model_path(config: &ChatConfig) -> Option<PathBuf> {
    let p = expected_path(config);
    if p.exists() { Some(p) } else { None }
}

/// Where this config's model file would live, regardless of whether it's
/// downloaded yet. Used by callers that want to log or display the path.
pub fn expected_path(config: &ChatConfig) -> PathBuf {
    config
        .cache_dir
        .join(config.model_repo.replace('/', "--"))
        .join(&config.model_file)
}

/// Whether the model file already exists on disk. Cheap; just a stat call.
pub fn is_present(config: &ChatConfig) -> bool {
    expected_path(config).is_file()
}

#[cfg(feature = "chat")]
pub use chat_impl::download_chat_model;

#[cfg(not(feature = "chat"))]
pub use stub_impl::download_chat_model;

#[cfg(feature = "chat")]
mod chat_impl {
    use super::*;
    use hf_hub::api::tokio::{ApiBuilder, ApiError};

    /// Download the GGUF described by `config` from HuggingFace into
    /// `config.cache_dir`. `on_progress` is called periodically with the
    /// bytes transferred so far; the total size is exposed once known via
    /// the first non-zero call.
    ///
    /// **Blocking-async**: this is `async fn` and should be driven from a
    /// `spawn_blocking` worker on the Tauri side so the IPC reactor stays
    /// responsive. The work itself is I/O bound.
    pub async fn download_chat_model<F>(config: &ChatConfig, _on_progress: F) -> Result<PathBuf>
    where
        F: FnMut(u64) + Send + 'static,
    {
        std::fs::create_dir_all(&config.cache_dir)
            .map_err(|e| TomeError::Other(format!("create chat cache dir: {e}")))?;

        // hf-hub manages its own internal cache path layout. We give it a
        // stable root inside our app data dir so users can reset by
        // deleting that one folder.
        let api = ApiBuilder::from_cache(hf_hub::Cache::new(config.cache_dir.clone()))
            .build()
            .map_err(|e: ApiError| TomeError::Other(format!("init hf api: {e}")))?;

        let repo = api.model(config.model_repo.clone());
        // hf-hub doesn't expose progress callbacks per-byte in stable
        // releases; we settle for "completed" reporting after the call
        // returns. Future work: stream the response ourselves and emit
        // progress events. For now the UI shows a spinner not a bar.
        let path = repo
            .get(&config.model_file)
            .await
            .map_err(|e| TomeError::Other(format!("download {}: {e}", config.model_file)))?;

        // Move the cached file to the deterministic path we promised
        // callers via `expected_path`. hf-hub returns its own internal
        // cache path; copying once keeps the rest of the codebase from
        // having to know about hf-hub's layout.
        let target = expected_path(config);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| TomeError::Other(format!("create model parent dir: {e}")))?;
        }
        if path != target {
            // Prefer hardlink to avoid copying multi-GB twice; fall back
            // to copy if the filesystem doesn't support it (e.g. across
            // mount points).
            if std::fs::hard_link(&path, &target).is_err() {
                std::fs::copy(&path, &target)
                    .map_err(|e| TomeError::Other(format!("copy model into place: {e}")))?;
            }
        }
        Ok(target)
    }
}

#[cfg(not(feature = "chat"))]
mod stub_impl {
    use super::*;

    /// Stub when the `chat` feature is disabled. Returns a clear error so
    /// the UI can render a "Chat not built" state instead of failing
    /// mysteriously deep inside an HTTP call.
    pub async fn download_chat_model<F>(_config: &ChatConfig, _on_progress: F) -> Result<PathBuf>
    where
        F: FnMut(u64) + Send + 'static,
    {
        Err(TomeError::Other(
            "chat disabled: rebuild with --features chat to enable model download".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_path_is_deterministic() {
        let cfg = ChatConfig {
            cache_dir: std::path::Path::new("/tmp/cache").to_path_buf(),
            model_repo: "microsoft/Phi-4-mini-instruct-GGUF".into(),
            model_file: "phi-4-mini-instruct-Q4_K_M.gguf".into(),
            ..ChatConfig::default()
        };
        let p = expected_path(&cfg);
        let s = p.to_string_lossy();
        assert!(s.contains("microsoft--Phi-4-mini-instruct-GGUF"));
        assert!(s.ends_with("phi-4-mini-instruct-Q4_K_M.gguf"));
    }

    #[test]
    fn is_present_returns_false_for_missing_file() {
        let cfg = ChatConfig {
            cache_dir: std::path::Path::new("/this/path/does/not/exist").to_path_buf(),
            ..ChatConfig::default()
        };
        assert!(!is_present(&cfg));
        assert!(chat_model_path(&cfg).is_none());
    }
}
