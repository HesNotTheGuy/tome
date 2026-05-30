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

/// A folder as described by an *import* (backup restore). Identified by
/// `name` rather than a numeric id, because ids are per-database and
/// meaningless across installs — name is the portable key. `parent_name`
/// links to another imported/existing folder by its name.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportFolder {
    pub name: String,
    pub parent_name: Option<String>,
    pub created_at: i64,
}

/// A bookmark as described by an import. References its folder by `name`
/// (or `None` for unfiled). `title` is the canonical article title — the
/// durable Wikipedia identity that survives dump re-ingests.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportBookmark {
    pub title: String,
    pub folder_name: Option<String>,
    pub note: Option<String>,
    pub created_at: i64,
}

/// Counts returned from [`crate::store::ArticleStore::import_bookmarks`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportOutcome {
    /// Folders newly created because no existing folder matched the name.
    pub folders_created: u64,
    /// Folders that matched an existing folder by name (reused, not duplicated).
    pub folders_matched: u64,
    /// Bookmarks inserted.
    pub bookmarks_added: u64,
    /// Bookmarks skipped because an identical (title, folder) pair already
    /// existed — re-importing the same backup is a safe no-op.
    pub bookmarks_skipped: u64,
}
