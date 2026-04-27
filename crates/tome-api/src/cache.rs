//! In-memory response cache.
//!
//! Wikipedia revisions are immutable, so a (title, revision) pair never needs
//! to be refetched once retrieved. We key by the request URL so caching works
//! for any GET regardless of the endpoint.
//!
//! On-disk caching with TTL is a follow-up; this in-memory layer is what
//! short-circuits repeated reads in the same session.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct Cache {
    entries: Mutex<HashMap<String, Vec<u8>>>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.entries
            .lock()
            .expect("cache mutex poisoned")
            .get(key)
            .cloned()
    }

    pub fn put(&self, key: impl Into<String>, body: Vec<u8>) {
        self.entries
            .lock()
            .expect("cache mutex poisoned")
            .insert(key.into(), body);
    }

    pub fn clear(&self) {
        self.entries.lock().expect("cache mutex poisoned").clear();
    }

    pub fn len(&self) -> usize {
        self.entries.lock().expect("cache mutex poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miss_then_hit() {
        let c = Cache::new();
        assert!(c.get("k").is_none());
        c.put("k", b"v".to_vec());
        assert_eq!(c.get("k"), Some(b"v".to_vec()));
    }

    #[test]
    fn put_overwrites() {
        let c = Cache::new();
        c.put("k", b"v1".to_vec());
        c.put("k", b"v2".to_vec());
        assert_eq!(c.get("k"), Some(b"v2".to_vec()));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn clear_empties() {
        let c = Cache::new();
        c.put("a", b"1".to_vec());
        c.put("b", b"2".to_vec());
        c.clear();
        assert!(c.is_empty());
    }
}
