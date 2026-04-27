//! Public query types.

use serde::{Deserialize, Serialize};
use tome_core::Tier;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub page_id: u64,
    pub title: String,
    pub tier: Tier,
    pub score: f32,
}
