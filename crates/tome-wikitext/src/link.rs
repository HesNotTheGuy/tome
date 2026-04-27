//! Internal-link resolution. Decoupled from storage so the renderer can be
//! tested without a real article store.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkStatus {
    /// The target article exists in our store; render as a live link.
    Available,
    /// The target is unknown locally; render with a "not in dump" marker.
    Missing,
    /// The target is a redirect to another title.
    Redirect(String),
}

pub trait LinkResolver: Send + Sync {
    fn resolve_internal(&self, target: &str) -> LinkStatus;
}

/// A resolver that treats every link as missing. Useful in tests when link
/// status is not the property under test.
pub struct NoopLinkResolver;

impl LinkResolver for NoopLinkResolver {
    fn resolve_internal(&self, _target: &str) -> LinkStatus {
        LinkStatus::Missing
    }
}

/// A resolver that treats every link as available. Useful for snapshot tests
/// of the renderer's "live link" output.
pub struct AllAvailableResolver;

impl LinkResolver for AllAvailableResolver {
    fn resolve_internal(&self, _target: &str) -> LinkStatus {
        LinkStatus::Available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_resolver_says_missing() {
        let r = NoopLinkResolver;
        assert_eq!(r.resolve_internal("Photon"), LinkStatus::Missing);
    }

    #[test]
    fn all_available_resolver_says_available() {
        let r = AllAvailableResolver;
        assert_eq!(r.resolve_internal("Photon"), LinkStatus::Available);
    }
}
