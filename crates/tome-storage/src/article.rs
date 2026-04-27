//! Article metadata and content shapes returned by the storage layer.

use serde::{Deserialize, Serialize};
use tome_core::Tier;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArticleMetadata {
    pub page_id: u64,
    pub title: String,
    pub tier: Tier,
    pub pinned: bool,
    /// Cold-tier articles record where to seek into the dump.
    pub stream_offset: Option<u64>,
    pub stream_length: Option<u64>,
    pub revision_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleRecord {
    pub metadata: ArticleMetadata,
    pub last_accessed: i64,
    pub access_count: u64,
}

/// Returned from [`ArticleStore::get_content`](crate::ArticleStore::get_content).
/// The caller dispatches on the variant: Hot/Warm yield content directly,
/// Cold tells the caller to resolve through the dump access layer, and
/// Evicted demands user confirmation before any fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArticleContent {
    Hot(String),
    Warm(String),
    Cold {
        stream_offset: u64,
        stream_length: Option<u64>,
    },
    Evicted,
}
