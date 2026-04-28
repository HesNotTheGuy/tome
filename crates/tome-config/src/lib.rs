//! Centralized configuration for Tome.
//!
//! All paths, URLs, defaults, and tunable values live here. Business logic in
//! other crates must never hardcode these — they read from a `Config` instance
//! injected at startup.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "Tome";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default User-Agent. The URL is the project's public issue tracker, used as
/// the contact channel required by MediaWiki's etiquette policy.
///
/// **Forking note:** if you ship a modified build, change this string so your
/// fork is identified as the source of its traffic, e.g.
/// `MyFork/1.0 (+https://github.com/you/myfork)`. Leaving it as
/// `Tome/1.0 (+...)` routes abuse reports to the wrong maintainer.
pub const DEFAULT_USER_AGENT: &str = "Tome/1.0 (+https://github.com/HesNotTheGuy/tome)";

/// Wikipedia REST endpoint for rendered article HTML (Parsoid output).
pub const WIKIPEDIA_REST_HTML_BASE: &str = "https://en.wikipedia.org/w/rest.php/v1/page";

/// Legacy MediaWiki action API endpoint.
pub const WIKIPEDIA_ACTION_API: &str = "https://en.wikipedia.org/w/api.php";

/// Hard ceiling on outbound API requests per second. Settings cannot raise this.
pub const MAX_REQUESTS_PER_SECOND: u32 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub data_dir: PathBuf,
    pub user_agent: String,
    pub requests_per_second: u32,
    pub kill_switch: bool,
}

impl Config {
    /// Build a default config rooted at the platform-appropriate user data dir.
    pub fn defaults() -> Self {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_NAME);
        Self {
            data_dir,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            requests_per_second: MAX_REQUESTS_PER_SECOND,
            kill_switch: false,
        }
    }
}

/// Persistent user-controlled settings.
///
/// Lives at `<data_dir>/settings.json`. Holds paths the user has chosen for
/// the dump and last-used index, plus future toggleable preferences. Loaded
/// at startup, written immediately on every change so an app crash never
/// loses the dump-path configuration.
///
/// All fields are `Option` so a fresh install starts blank and the UI can
/// surface "not configured yet" states clearly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// Absolute path to the multistream bz2 dump file. Required for Cold-tier
    /// reads.
    #[serde(default)]
    pub dump_path: Option<PathBuf>,
    /// Absolute path to the last index file the user ingested. Used only as
    /// a UI convenience (pre-fills the ingest input).
    #[serde(default)]
    pub last_index_path: Option<PathBuf>,
    /// Whether the Reader surfaces a "Related articles" section based on
    /// shared categorylinks. Default on; toggleable in Settings.
    #[serde(default = "default_true")]
    pub recommendations_enabled: bool,
    /// Absolute path to a `.pmtiles` archive used as the offline basemap on
    /// the Map pane. When `None`, the Map falls back to live OSM tiles.
    #[serde(default)]
    pub map_source_path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dump_path: None,
            last_index_path: None,
            recommendations_enabled: true,
            map_source_path: None,
        }
    }
}

fn default_true() -> bool {
    true
}

impl Settings {
    pub fn settings_file(data_dir: &std::path::Path) -> PathBuf {
        data_dir.join("settings.json")
    }

    /// Load from `<data_dir>/settings.json`. Missing file → default.
    /// Corrupt file → default + log (non-fatal so the app always starts).
    pub fn load(data_dir: &std::path::Path) -> Self {
        let path = Self::settings_file(data_dir);
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Atomic-ish write: write to a temp file then rename, so a crash
    /// mid-write doesn't leave a half-written settings.json.
    pub fn save(&self, data_dir: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let path = Self::settings_file(data_dir);
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn load_returns_default_when_file_absent() {
        let dir = tempdir().unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let s = Settings {
            dump_path: Some(PathBuf::from("/some/dump.bz2")),
            last_index_path: Some(PathBuf::from("/some/index.bz2")),
            recommendations_enabled: false,
            map_source_path: Some(PathBuf::from("/maps/world.pmtiles")),
        };
        s.save(dir.path()).unwrap();
        let loaded = Settings::load(dir.path());
        assert_eq!(loaded, s);
    }

    #[test]
    fn corrupt_file_falls_back_to_default() {
        let dir = tempdir().unwrap();
        std::fs::write(Settings::settings_file(dir.path()), b"not valid json").unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(s, Settings::default());
    }
}
