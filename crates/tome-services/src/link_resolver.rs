//! Link resolver backed by the article store.
//!
//! When the wikitext renderer encounters `[[Photon]]`, it asks this resolver
//! whether "Photon" is in the local store; if so, render as a live link, if
//! not, mark as missing for the UI to style.

use std::sync::Arc;

use tome_core::Title;
use tome_storage::ArticleStore;
use tome_wikitext::link::{LinkResolver, LinkStatus};

pub struct StorageLinkResolver {
    storage: Arc<dyn ArticleStore>,
}

impl StorageLinkResolver {
    pub fn new(storage: Arc<dyn ArticleStore>) -> Self {
        Self { storage }
    }
}

impl LinkResolver for StorageLinkResolver {
    fn resolve_internal(&self, target: &str) -> LinkStatus {
        let title = Title::new(target);
        match self.storage.lookup(&title) {
            Ok(Some(_)) => LinkStatus::Available,
            _ => LinkStatus::Missing,
        }
    }
}
