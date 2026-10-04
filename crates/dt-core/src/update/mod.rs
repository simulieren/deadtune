//! Self-update. A release publishes `latest.json` (a [`Manifest`]) next to the raw
//! executable and its minisign signature. The app fetches the manifest, [`decide`]s
//! whether to offer it, downloads the asset, [`verify`]s size, SHA-256 and the
//! signature against the embedded release key, then [`install`]s it by renaming the
//! running executable aside. The signed trusted comment binds the signature to one
//! version, commit and target, so an older or foreign signed file is rejected.

use std::fmt;
use std::str::FromStr;

mod install;
pub use install::{cleanup, install};

#[cfg(feature = "fetch")]
mod fetch;
#[cfg(feature = "fetch")]
pub use fetch::{download, fetch_manifest};

#[cfg(test)]
mod tests;

/// Public half of the release key; the private half never enters the repo.
pub const RELEASE_PUBKEY: &str = include_str!("release.pub");

pub const STABLE_MANIFEST_URL: &str =
    "https://github.com/simulieren/deadtune/releases/latest/download/latest.json";
pub const TESTING_MANIFEST_URL: &str =
    "https://github.com/simulieren/deadtune/releases/download/testing/latest.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    #[default]
    Stable,
    Testing,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    WindowsX64,
    LinuxX64,
}

impl Target {
    /// The target this binary was built for; `None` where updates are not published.
    pub const CURRENT: Option<Target> = if cfg!(all(windows, target_arch = "x86_64")) {
        Some(Target::WindowsX64)
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(Target::LinuxX64)
    } else {
        None
    };
}

/// `MAJOR.MINOR.PATCH`, ordered numerically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u64, pub u64, pub u64);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

impl FromStr for Version {
    type Err = UpdateError;

    /// Exactly three dot-separated runs of ASCII digits; no `v`, sign or suffix.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || UpdateError::Manifest(format!("version {s:?} is not MAJOR.MINOR.PATCH"));
        let mut parts = s.split('.').map(|p| {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            p.parse::<u64>().map_err(|_| bad())
        });
        match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(a), Some(b), Some(c), None) => Ok(Version(a?, b?, c?)),
            _ => Err(bad()),
        }
    }
}

impl serde::Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Asset {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// Full `.minisig` file contents.
    pub minisig: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub channel: Channel,
    /// Serialized as "0.3.0".
    pub version: Version,
    pub commit: String,
    /// RFC 3339.
    pub published: String,
    pub notes_url: String,
    pub assets: std::collections::BTreeMap<Target, Asset>,
}

/// What this running binary is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Current {
    pub version: Version,
    /// Embedded at build time from `DEADTUNE_COMMIT`; `None` for dev builds.
    pub commit: Option<String>,
    pub target: Target,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offer {
    UpToDate,
    /// The user chose "skip this version" for exactly this one.
    Skipped,
    Available {
        version: Version,
        commit: String,
        notes_url: String,
        asset: Asset,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("download is {got} bytes, expected {expected}")]
    Size { got: u64, expected: u64 },
    #[error("download checksum mismatch")]
    Checksum,
    #[error("signature: {0}")]
    Signature(String),
    #[error("signed for {got:?}, expected {expected:?}")]
    WrongRelease { got: String, expected: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(feature = "fetch")]
    #[error("download: {0}")]
    Http(#[from] ureq::Error),
}

impl Current {
    /// This build: package version, `DEADTUNE_COMMIT`, [`Target::CURRENT`].
    pub fn this_build() -> Option<Current> {
        Some(Current {
            version: env!("CARGO_PKG_VERSION")
                .parse()
                .expect("package version is MAJOR.MINOR.PATCH"),
            commit: option_env!("DEADTUNE_COMMIT").map(str::to_owned),
            target: Target::CURRENT?,
        })
    }
}

/// Exactly what the release script passes to `minisign -t`.
pub fn trusted_comment(version: Version, commit: &str, target: Target) -> String {
    let target = match target {
        Target::WindowsX64 => "windows-x64",
        Target::LinuxX64 => "linux-x64",
    };
    format!("deadtune {version} {commit} {target}")
}

/// Stable: offer a strictly newer version. Testing: offer any build whose commit
/// differs from ours and whose version is not older. Never offer a downgrade.
pub fn decide(current: &Current, manifest: &Manifest, skipped: Option<Version>) -> Offer {
    let Some(asset) = manifest.assets.get(&current.target) else {
        return Offer::UpToDate;
    };
    let newer = manifest.version > current.version;
    let offered = match manifest.channel {
        Channel::Stable => newer,
        // A dev build (no commit) only hears about strictly newer versions.
        Channel::Testing => {
            newer
                || (manifest.version == current.version
                    && current
                        .commit
                        .as_deref()
                        .is_some_and(|c| c != manifest.commit))
        }
    };
    if !offered {
        Offer::UpToDate
    } else if skipped == Some(manifest.version) {
        Offer::Skipped
    } else {
        Offer::Available {
            version: manifest.version,
            commit: manifest.commit.clone(),
            notes_url: manifest.notes_url.clone(),
            asset: asset.clone(),
        }
    }
}

/// Size, SHA-256, minisign signature by `pubkey`, and the trusted comment equal to
/// `trusted_comment(version, commit, target)`. Nothing is written before this passes.
pub fn verify(
    bytes: &[u8],
    asset: &Asset,
    version: Version,
    commit: &str,
    target: Target,
    pubkey: &str,
) -> Result<(), UpdateError> {
    let got = bytes.len() as u64;
    if got != asset.size {
        return Err(UpdateError::Size {
            got,
            expected: asset.size,
        });
    }
    if !crate::backup::sha256_hex(bytes).eq_ignore_ascii_case(&asset.sha256) {
        return Err(UpdateError::Checksum);
    }
    let sig_err = |e: minisign_verify::Error| UpdateError::Signature(e.to_string());
    let key = minisign_verify::PublicKey::decode(pubkey).map_err(sig_err)?;
    let signature = minisign_verify::Signature::decode(&asset.minisig).map_err(sig_err)?;
    key.verify(bytes, &signature, false).map_err(sig_err)?;
    let expected = trusted_comment(version, commit, target);
    if signature.trusted_comment() != expected {
        return Err(UpdateError::WrongRelease {
            got: signature.trusted_comment().to_owned(),
            expected,
        });
    }
    Ok(())
}
