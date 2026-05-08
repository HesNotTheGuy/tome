//! Bookmarks and folders.
//!
//! Bookmarks are explicit user choices to save an article for later, separate
//! from the tier system (Hot/Warm/Cold/Evicted) which is about *where* an
//! article's content lives. Both can apply to the same article.
//!
//! Folders are flat for the v1 UI (one level — no nested folders), but the
//! schema's `parent_id` already supports nesting if we surface it later.

use serde::{Deserialize, Serialize};

/// A single bookmark. `folder_id = None` means the bookmark lives at the
/// root ("unfiled"). `note` is optional free text the user typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: i64,
    pub article_title: String,
    pub folder_id: Option<i64>,
    pub note: Option<String>,
    pub created_at: i64,
}

/// A folder of bookmarks. `parent_id = None` means a root-level folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookmarkFolder {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub created_at: i64,
}
