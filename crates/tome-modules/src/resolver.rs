//! Category resolver trait. Real implementation lives in `tome-services`,
//! where the MediaWiki API client is composed; this trait is what
//! `tome-modules` depends on so it never reaches across to API land.

use async_trait::async_trait;
use tome_core::Result;

#[async_trait]
pub trait CategoryResolver: Send + Sync {
    /// Return all article titles inside the given category, recursing into
    /// subcategories up to `depth`. Depth 0 = exact category only.
    async fn resolve(&self, category: &str, depth: u8) -> Result<Vec<String>>;
}

/// A resolver that always returns an empty list. Useful in tests where the
/// resolution result isn't the property under test.
pub struct NoopResolver;

#[async_trait]
impl CategoryResolver for NoopResolver {
    async fn resolve(&self, _category: &str, _depth: u8) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}
