//! Title-redirect records.
//!
//! Sourced from Wikipedia's `redirect` table. A redirect maps a page that
//! exists only as a "go to X" stub to its actual target. We store the
//! source page id and the resolved target title; the namespace and
//! interwiki/fragment columns are filtered at parse time so only
//! main-namespace local redirects land here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redirect {
    pub from_page_id: u64,
    /// The redirect's destination article title, with underscores
    /// normalized to spaces (Wikipedia URL form → display form).
    pub target_title: String,
}
