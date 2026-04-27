use serde::{Deserialize, Serialize};

/// Normalized Wikipedia article title. Spaces and underscores are equivalent
/// on Wikipedia; we canonicalize to spaces internally.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Title(String);

impl Title {
    pub fn new(raw: impl Into<String>) -> Self {
        let s: String = raw.into();
        Self(s.replace('_', " ").trim().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for Title {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
