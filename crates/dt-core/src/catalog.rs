//! ConVar metadata. `catalog/catalog.toml` is generated from
//! `research/data/convar_catalog.csv` plus `catalog/curated.toml` and embedded at build.

use std::collections::BTreeMap;

use crate::preset::PresetId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyClass {
    Live,
    LiveCheat,
    Restart,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Kind {
    Bool,
    Int,
    Float,
    Enum { options: Vec<String> },
    String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Impact {
    High,
    Medium,
    Low,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CatalogEntry {
    pub category: String,
    #[serde(flatten)]
    pub kind: Kind,
    pub range: Option<[f64; 2]>,
    pub step: Option<f64>,
    pub default: Option<String>,
    pub apply: ApplyClass,
    #[serde(default)]
    pub impact: Impact,
    #[serde(default)]
    pub denylist: bool,
    #[serde(default)]
    pub notes: String,
    /// Raw value per preset; a leading `//` means present but commented out.
    #[serde(default)]
    pub presets: BTreeMap<PresetId, String>,
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub entries: BTreeMap<String, CatalogEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("csv: {0}")]
    Csv(#[from] csv::Error),
    #[error("toml: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml: {0}")]
    TomlSer(#[from] toml::ser::Error),
}

impl Catalog {
    pub fn embedded() -> &'static Catalog {
        todo!()
    }

    pub fn from_toml(text: &str) -> Result<Catalog, CatalogError> {
        let _ = text;
        todo!()
    }

    pub fn get(&self, name: &str) -> Option<&CatalogEntry> {
        self.entries.get(name)
    }

    /// Case-insensitive match on name, category and notes.
    pub fn search<'a>(&'a self, query: &'a str) -> impl Iterator<Item = (&'a str, &'a CatalogEntry)> + 'a {
        let _ = query;
        std::iter::empty()
    }

    pub fn categories(&self) -> Vec<&str> {
        todo!()
    }

    /// Unlisted convars are allowed but treated as `Restart`.
    pub fn apply_class(&self, name: &str) -> ApplyClass {
        self.get(name).map_or(ApplyClass::Restart, |e| e.apply)
    }

    pub fn is_denied(&self, name: &str) -> bool {
        self.get(name).is_some_and(|e| e.denylist)
    }
}

/// Builds catalog.toml text from the research CSV and the hand-curated overlay.
pub fn generate(csv: &str, curated: &str) -> Result<String, CatalogError> {
    let _ = (csv, curated);
    todo!()
}
