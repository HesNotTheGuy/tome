//! Geographic coordinates for articles.
//!
//! Sourced from Wikipedia's `geo_tags` table. The full schema has many fields
//! (gt_id, gt_globe, gt_dim, gt_country, gt_region, …); we keep only what's
//! useful in-app and discard the rest. `kind` is the gt_type column and gives
//! a hint about the kind of place (`city`, `mountain`, `landmark`, etc).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Geotag {
    pub page_id: u64,
    pub lat: f64,
    pub lon: f64,
    /// True for the article's primary coordinate. Some articles have several
    /// (e.g. a city with separate downtown / neighborhood pins); we usually
    /// only display the primary.
    pub primary: bool,
    pub kind: Option<String>,
}
