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
