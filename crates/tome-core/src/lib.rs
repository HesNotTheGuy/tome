//! Shared types, errors, and traits used across all Tome crates.

pub mod config;
pub mod error;
pub mod searcher;
pub mod tier;
pub mod title;

pub use config::{
    APP_NAME, APP_VERSION, Config, DEFAULT_USER_AGENT, MAX_REQUESTS_PER_SECOND, Settings,
    WIKIPEDIA_ACTION_API, WIKIPEDIA_REST_HTML_BASE,
};
pub use error::{Result, TomeError};
pub use searcher::{SearchHit, Searcher};
pub use tier::Tier;
pub use title::Title;
