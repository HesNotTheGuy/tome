//! Category membership records.
//!
//! Sourced from Wikipedia's `categorylinks` table. Each row says "page X
//! belongs to category Y, as a `cl_type`". We keep the four columns we need
//! for browsing — page id, category name, type — and discard sortkeys and
//! timestamps which are display hints we don't use.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CategoryMemberKind {
    /// A regular article (member of the category as content).
    Page,
    /// A subcategory.
    Subcat,
    /// A media file (image, audio, etc).
    File,
}

impl CategoryMemberKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CategoryMemberKind::Page => "page",
            CategoryMemberKind::Subcat => "subcat",
            CategoryMemberKind::File => "file",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "page" => Some(Self::Page),
            "subcat" => Some(Self::Subcat),
            "file" => Some(Self::File),
            _ => None,
        }
    }
}

/// One row in the categorylinks table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryLink {
    pub from_page_id: u64,
    pub category: String,
    pub kind: CategoryMemberKind,
}

/// Result of listing what's in a category. Article members carry their
/// title (joined from the articles table); subcategories carry only the
/// category name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryMember {
    pub kind: CategoryMemberKind,
    /// The displayable name. For pages, this is the article title. For
    /// subcategories, the subcategory name with underscores normalized to
    /// spaces.
    pub title: String,
    pub page_id: u64,
}
