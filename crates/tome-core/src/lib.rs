//! Shared types, errors, and traits used across all Tome crates.

pub mod error;
pub mod tier;
pub mod title;

pub use error::{Result, TomeError};
pub use tier::Tier;
pub use title::Title;
