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
///
/// When `explicit_model_path` is set, that path is returned verbatim: the
/// user told us exactly where the model lives (side-loaded from a USB
/// drive or network share), so the download cache layout is irrelevant —
/// the explicit path IS the model location.
pub fn expected_path(config: &ChatConfig) -> PathBuf {
    if let Some(explicit) = &config.explicit_model_path {
        return explicit.clone();
    }
    config
        .cache_dir
        .join(config.model_repo.replace('/', "--"))
        .join(&config.model_file)
}

/// Whether the model file already exists on disk. Cheap; just a stat call.
pub fn is_present(config: &ChatConfig) -> bool {
    expected_path(config).is_file()
}

/// Honor a user-supplied `explicit_model_path` ahead of any download logic.
///
/// Returns `None` when no explicit path is configured — the caller falls
/// through to its normal download path. Otherwise the answer never touches
/// the network: the file's path and byte size when it exists, or a
/// user-actionable error when it doesn't.
fn check_explicit_model(config: &ChatConfig) -> Option<Result<(PathBuf, u64)>> {
    let explicit = config.explicit_model_path.as_ref()?;
    match std::fs::metadata(explicit) {
        Ok(meta) if meta.is_file() => Some(Ok((explicit.clone(), meta.len()))),
        _ => Some(Err(TomeError::Other(format!(
            "chat model not found at {} — check the path in Settings → Ask Tome, \
             or clear it to use the downloader",
            explicit.display()
        )))),
    }
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
    /// `config.cache_dir`. `on_progress` receives bytes-on-disk readings
    /// during the download — see the polling loop below for the cadence.
    ///
    /// **Blocking-async**: this is `async fn` and should be driven from a
    /// `spawn_blocking` worker on the Tauri side so the IPC reactor stays
    /// responsive. The work itself is I/O bound.
    pub async fn download_chat_model<F>(config: &ChatConfig, mut on_progress: F) -> Result<PathBuf>
    where
        F: FnMut(u64) + Send + 'static,
    {
        // A side-loaded model wins outright: no network, no hf-hub API
        // construction. One progress tick with the real size so the UI's
        // byte counter lands on the truth immediately.
        if let Some(resolved) = check_explicit_model(config) {
            let (path, size) = resolved?;
            on_progress(size);
            return Ok(path);
        }

        std::fs::create_dir_all(&config.cache_dir)
            .map_err(|e| TomeError::Other(format!("create chat cache dir: {e}")))?;

        // hf-hub manages its own internal cache path layout. We give it a
        // stable root inside our app data dir so users can reset by
        // deleting that one folder.
        let api = ApiBuilder::from_cache(hf_hub::Cache::new(config.cache_dir.clone()))
            .build()
            .map_err(|e: ApiError| TomeError::Other(format!("init hf api: {e}")))?;

        let repo = api.model(config.model_repo.clone());

        // hf-hub doesn't expose per-byte progress callbacks in its stable
        // API. To still give the UI something to display during a multi-GB
        // download, we spawn a polling task that stats hf-hub's working
        // directory once per second and emits the largest in-progress file
        // size we find. Crude, but real numbers — the UI sees actual bytes
        // landing on disk rather than the previous "0 MB the whole time"
        // bug. A future commit can replace this with a streaming reqwest
        // download for proper bytes-transferred + total-size events.
        let cache_root = config.cache_dir.clone();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let progress_handle = tokio::spawn(async move {
            let mut last = 0_u64;
            loop {
                tokio::select! {
                    _ = &mut cancel_rx => break,
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
                }
                let size = walking_largest_file(&cache_root).unwrap_or(0);
                if size > last {
                    last = size;
                    on_progress(size);
                }
            }
            // Final tick after cancel so the UI sees the completed size.
            let final_size = walking_largest_file(&cache_root).unwrap_or(last);
            on_progress(final_size);
        });

        let download_result = repo.get(&config.model_file).await;

        // Stop the polling task and await its final emission on BOTH the
        // success and error paths — capture the result first, tear the
        // task down deterministically, then propagate. Using `?` directly
        // on repo.get() would skip this teardown on error and leave the
        // poller to terminate only via cancel_tx's drop (one stray stat).
        let _ = cancel_tx.send(());
        let _ = progress_handle.await;

        let path = download_result.map_err(|e| {
            TomeError::Other(format!(
                "download {}: {e} — if this machine is offline, download {} from \
                 huggingface.co/{} on a connected machine, copy it to this machine, \
                 and set its path in Settings → Ask Tome (see docs/OFFLINE-SURVIVAL-KIT.md)",
                config.model_file, config.model_file, config.model_repo
            ))
        })?;

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

    /// Walk the cache root and return the biggest single regular file we
    /// see. hf-hub places the partial GGUF into a subdirectory; we don't
    /// need to know exactly where, just "what's the largest in-flight
    /// thing" so the UI has a credible byte counter. Returns `None` if
    /// the directory doesn't exist yet.
    fn walking_largest_file(root: &std::path::Path) -> Option<u64> {
        fn visit(dir: &std::path::Path, max: &mut u64) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                let p = e.path();
                if ft.is_dir() {
                    visit(&p, max);
                } else if ft.is_file()
                    && let Ok(m) = e.metadata()
                {
                    let s = m.len();
                    if s > *max {
                        *max = s;
                    }
                }
            }
        }
        if !root.exists() {
            return None;
        }
        let mut max = 0_u64;
        visit(root, &mut max);
        Some(max)
    }
}

#[cfg(not(feature = "chat"))]
mod stub_impl {
    use super::*;

    /// Stub when the `chat` feature is disabled. Returns a clear error so
    /// the UI can render a "Chat not built" state instead of failing
    /// mysteriously deep inside an HTTP call.
    ///
    /// A user-supplied `explicit_model_path` is still honored: serving a
    /// file that's already on disk needs no download machinery, so an
    /// offline user on a stock build with a side-loaded model gets a
    /// working path instead of "rebuild with --features chat".
    pub async fn download_chat_model<F>(config: &ChatConfig, mut on_progress: F) -> Result<PathBuf>
    where
        F: FnMut(u64) + Send + 'static,
    {
        if let Some(resolved) = check_explicit_model(config) {
            let (path, size) = resolved?;
            on_progress(size);
            return Ok(path);
        }
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

    fn explicit_cfg(path: PathBuf) -> ChatConfig {
        ChatConfig {
            explicit_model_path: Some(path),
            ..ChatConfig::default()
        }
    }

    #[test]
    fn explicit_path_overrides_cache_layout() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("side-loaded.gguf");
        std::fs::write(&model, b"fake gguf bytes").unwrap();

        let cfg = explicit_cfg(model.clone());
        assert_eq!(expected_path(&cfg), model);
        assert!(is_present(&cfg));
        assert_eq!(chat_model_path(&cfg), Some(model));
    }

    #[test]
    fn explicit_path_missing_file_reports_absent() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.gguf");

        let cfg = explicit_cfg(missing.clone());
        assert_eq!(expected_path(&cfg), missing);
        assert!(!is_present(&cfg));
        assert!(chat_model_path(&cfg).is_none());
    }

    /// Runs against whichever `download_chat_model` is compiled in: both
    /// the stub and the real implementation must honor an existing
    /// explicit file before any feature gate or network logic.
    #[tokio::test]
    async fn download_returns_explicit_file_with_one_size_progress_tick() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("side-loaded.gguf");
        std::fs::write(&model, b"0123456789").unwrap(); // 10 bytes

        let cfg = explicit_cfg(model.clone());
        let ticks: std::sync::Arc<std::sync::Mutex<Vec<u64>>> = Default::default();
        let sink = ticks.clone();
        let path = download_chat_model(&cfg, move |bytes| {
            sink.lock().unwrap().push(bytes);
        })
        .await
        .unwrap();

        assert_eq!(path, model);
        assert_eq!(
            *ticks.lock().unwrap(),
            vec![10],
            "exactly one progress tick with the file's byte size"
        );
    }

    #[tokio::test]
    async fn download_errors_clearly_for_missing_explicit_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("usb-was-unplugged.gguf");

        let cfg = explicit_cfg(missing.clone());
        let err = download_chat_model(&cfg, |_| {}).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("chat model not found at"), "got: {msg}");
        assert!(
            msg.contains(&missing.display().to_string()),
            "error must name the configured path; got: {msg}"
        );
        assert!(msg.contains("Settings"), "got: {msg}");
        assert!(
            msg.contains("clear it to use the downloader"),
            "got: {msg}"
        );
    }

    /// Without an explicit path the stub's behavior is unchanged: a clear
    /// feature-disabled error, no filesystem probing of the cache layout.
    #[cfg(not(feature = "chat"))]
    #[tokio::test]
    async fn stub_download_without_explicit_path_reports_feature_disabled() {
        let cfg = ChatConfig {
            cache_dir: std::path::Path::new("/this/path/does/not/exist").to_path_buf(),
            ..ChatConfig::default()
        };
        let err = download_chat_model(&cfg, |_| {}).await.unwrap_err();
        assert!(err.to_string().contains("--features chat"));
    }
}
