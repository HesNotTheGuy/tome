//! Module definition format.
//!
//! Modules are portable: the same TOML round-trips through serde and is
//! re-importable into any Tome install.

use serde::{Deserialize, Serialize};
use tome_core::{Result, Tier, TomeError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleSpec {
    /// Unique kebab-case identifier (e.g. `"science-basics"`).
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub default_tier: Tier,
    #[serde(default)]
    pub categories: Vec<CategorySpec>,
    #[serde(default)]
    pub explicit_titles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategorySpec {
    pub name: String,
    /// 0 = exact category only; 1 = category + immediate subcats; etc.
    pub depth: u8,
}

impl ModuleSpec {
    pub fn from_toml(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(|e| TomeError::Other(format!("module toml parse: {e}")))
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| TomeError::Other(format!("module toml serialize: {e}")))
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(TomeError::Other("module id is empty".into()));
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(TomeError::Other(format!(
                "module id '{}' must be ascii kebab-case",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(TomeError::Other("module name is empty".into()));
        }
        if self.categories.is_empty() && self.explicit_titles.is_empty() {
            return Err(TomeError::Other(format!(
                "module '{}' defines neither categories nor explicit_titles",
                self.id
            )));
        }
        for cat in &self.categories {
            if cat.depth > 10 {
                return Err(TomeError::Other(format!(
                    "category '{}' depth {} exceeds safe limit (10)",
                    cat.name, cat.depth
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ModuleSpec {
        ModuleSpec {
            id: "science-basics".into(),
            name: "Science Basics".into(),
            description: Some("Introductory physics, chemistry, and biology.".into()),
            default_tier: Tier::Warm,
            categories: vec![
                CategorySpec {
                    name: "Physics".into(),
                    depth: 2,
                },
                CategorySpec {
                    name: "Chemistry".into(),
                    depth: 2,
                },
            ],
            explicit_titles: vec!["Scientific method".into()],
        }
    }

    #[test]
    fn toml_round_trip() {
        let original = sample();
        let serialized = original.to_toml().unwrap();
        let parsed = ModuleSpec::from_toml(&serialized).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn from_toml_accepts_minimal_input() {
        let text = r#"
            id = "tiny"
            name = "Tiny module"
            default_tier = "cold"
            categories = []
            explicit_titles = ["Photon"]
        "#;
        let spec = ModuleSpec::from_toml(text).unwrap();
        assert_eq!(spec.id, "tiny");
        assert_eq!(spec.default_tier, Tier::Cold);
        assert_eq!(spec.explicit_titles, vec!["Photon".to_string()]);
    }

    #[test]
    fn validate_rejects_empty_id() {
        let mut s = sample();
        s.id = String::new();
        assert!(s.validate().is_err());
    }

    #[test]
    fn validate_rejects_non_kebab_id() {
        let mut s = sample();
        s.id = "Science Basics".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn validate_rejects_module_with_no_content() {
        let mut s = sample();
        s.categories.clear();
        s.explicit_titles.clear();
        assert!(s.validate().is_err());
    }

    #[test]
    fn validate_rejects_excessive_depth() {
        let mut s = sample();
        s.categories[0].depth = 50;
        assert!(s.validate().is_err());
    }

    #[test]
    fn validate_accepts_well_formed() {
        let s = sample();
        s.validate().unwrap();
    }
}
