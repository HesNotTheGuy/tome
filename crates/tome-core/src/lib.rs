//! Shared types, errors, and traits used across all Tome crates.

pub mod error;
pub mod searcher;
pub mod tier;
pub mod title;

pub use error::{Result, TomeError};
pub use searcher::{SearchHit, Searcher};
pub use tier::Tier;
pub use title::Title;
