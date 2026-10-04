//! A profile = base preset + personal overrides, stored as TOML.
//! Also imports/exports Sqooky's `overrides.gi` format.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::gi::Overrides;
use crate::preset::PresetId;

/// Serialized as `"kaiz_minspec"` or `"file:<path>"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseRef {
    Preset(PresetId),
    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ConVarEdits {
    #[serde(default)]
    pub set: BTreeMap<String, String>,
    #[serde(default)]
    pub comment: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    pub name: String,
    pub base: BaseRef,
    pub base_rev: Option<String>,
    #[serde(default)]
    pub convars: ConVarEdits,
    #[serde(default)]
    pub video: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("toml: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("overrides.gi line {line}: {msg}")]
    OverridesSyntax { line: usize, msg: String },
}

impl serde::Serialize for BaseRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let _ = s;
        todo!()
    }
}

impl<'de> serde::Deserialize<'de> for BaseRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let _ = d;
        todo!()
    }
}

impl Profile {
    pub fn from_toml(text: &str) -> Result<Profile, ProfileError> {
        let _ = text;
        todo!()
    }

    pub fn to_toml(&self) -> Result<String, ProfileError> {
        todo!()
    }

    pub fn overrides(&self) -> Overrides {
        todo!()
    }
}

pub fn parse_overrides_gi(text: &str) -> Result<Overrides, ProfileError> {
    let _ = text;
    todo!()
}

pub fn write_overrides_gi(overrides: &Overrides) -> String {
    let _ = overrides;
    todo!()
}

/// "Plugged in" and "Battery".
pub fn builtin_suggestions() -> Vec<Profile> {
    todo!()
}
