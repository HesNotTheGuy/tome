//! Cross-platform disk-space inspection.
//!
//! Used as a pre-flight check before multi-GB downloads (the chat model,
//! the embedding model, future map / dump downloads). The policy is
//! **warn-only, never block** — Tome doesn't refuse to use your machine,
//! but it tells you up front when a download would leave the disk under
//! the 15% free-space threshold most filesystems prefer for stable
//! performance.
//!
//! 15% is the consensus number across NTFS, APFS, and ext4 — all three
//! degrade in different ways once free space drops below that.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Below this fraction of total disk free *after* a download, we surface
/// a warning. Windows NTFS recommends 15% for defrag stability; APFS
/// likes 15-20% for snapshots; ext4 reserves 5% and fragments under 10%.
/// 15% is a defensible cross-platform floor.
pub const RECOMMENDED_MIN_FREE_PCT: u8 = 15;

/// Snapshot of disk space at `path`, plus a projected post-download
/// state. The frontend uses this to render a warning modal before
/// kicking off a download.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskSpaceCheck {
    /// Bytes currently free on the volume containing `path`.
    pub free_bytes: u64,
    /// Total capacity of that volume.
    pub total_bytes: u64,
    /// Bytes the caller intends to download. Echoed back so the UI has
    /// everything it needs without re-passing the value.
    pub required_bytes: u64,
    /// Free space remaining after the download, as a percentage of total.
    /// Computed server-side so the frontend doesn't have to mirror the math.
    pub free_after_download_pct: f32,
    /// The threshold below which we warn. Surfaced so the frontend can
    /// say "below the recommended N%" without hardcoding the number.
    pub recommended_min_pct: u8,
    /// Whether a warning should be shown. True if
    /// `free_after_download_pct < recommended_min_pct` OR if the
    /// download wouldn't fit at all (post-download free < 0 in u64 math
    /// => free_bytes < required_bytes).
    pub warn: bool,
}

/// Inspect the volume containing `path` and project what would remain
/// after a `bytes_required`-sized download.
///
/// Errors only on platform-level I/O failures (no readable path); a
/// "would overfill the disk" situation is reported as `warn = true`
/// with the same fields populated, not as an error. Callers can show
/// the user a "this won't fit" message instead of a stack trace.
pub fn check_disk_space(path: &Path, bytes_required: u64) -> std::io::Result<DiskSpaceCheck> {
    // fs4 wants an existing path. If the caller passed something that
    // doesn't exist yet (a not-yet-created cache dir, common on first
    // run), walk up to the nearest existing ancestor so the volume
    // resolution still works.
    let probe = nearest_existing(path);
    let free_bytes = fs4::available_space(&probe)?;
    let total_bytes = fs4::total_space(&probe)?;

    let post_free = free_bytes.saturating_sub(bytes_required);
    let pct = if total_bytes > 0 {
        (post_free as f64 / total_bytes as f64 * 100.0) as f32
    } else {
        0.0
    };
    let warn = free_bytes < bytes_required || (pct as u8) < RECOMMENDED_MIN_FREE_PCT;

    Ok(DiskSpaceCheck {
        free_bytes,
        total_bytes,
        required_bytes: bytes_required,
        free_after_download_pct: pct,
        recommended_min_pct: RECOMMENDED_MIN_FREE_PCT,
        warn,
    })
}

/// Walk up `path`'s ancestors until one exists, so disk queries against
/// not-yet-created paths still resolve to the right volume.
fn nearest_existing(path: &Path) -> std::path::PathBuf {
    let mut p = path.to_path_buf();
    while !p.exists() {
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
    if p.as_os_str().is_empty() {
        // Last resort: current dir.
        std::path::PathBuf::from(".")
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_against_a_real_dir_returns_sane_numbers() {
        let dir = std::env::temp_dir();
        let check = check_disk_space(&dir, 0).expect("query temp dir");
        assert!(check.total_bytes > 0, "every real disk has nonzero total");
        assert!(check.free_bytes <= check.total_bytes);
        // With 0 bytes required, free_after equals free.
        assert!(check.free_after_download_pct >= 0.0);
        assert!(check.free_after_download_pct <= 100.0);
    }

    #[test]
    fn nearest_existing_walks_up_to_a_real_dir() {
        let weird = std::env::temp_dir().join("does/not/exist/yet/zzz.tmp");
        let found = nearest_existing(&weird);
        assert!(found.exists(), "{found:?}");
    }

    #[test]
    fn over_full_request_sets_warn_true() {
        let dir = std::env::temp_dir();
        // 1 PiB — guaranteed to exceed any test machine's free space.
        let check = check_disk_space(&dir, 1_125_899_906_842_624).expect("query");
        assert!(check.warn, "1 PiB request must trip the warning");
    }

    #[test]
    fn small_request_below_threshold_does_not_warn() {
        let dir = std::env::temp_dir();
        // 1 KiB — should never warn on a normal dev machine.
        // Only false if the test machine is already pathologically full,
        // which would be a separate problem.
        let check = check_disk_space(&dir, 1024).expect("query");
        if check.free_after_download_pct >= RECOMMENDED_MIN_FREE_PCT as f32 {
            assert!(!check.warn);
        }
    }
}
