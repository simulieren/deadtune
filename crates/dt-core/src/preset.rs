//! Known community presets. GPL presets are embedded; presets whose licence forbids
//! redistribution are [`Remote`]: the player downloads or imports the author's file
//! (see [`remote`]).

use std::path::{Path, PathBuf};

pub mod remote;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PresetId {
    Vanilla,
    Sqooky,
    SqookyTest,
    KaizMinspec,
    KaizExtremelow,
    BootMaxfps,
    OptilockRecommended,
    OptilockPotato,
    #[serde(rename = "sidelock")]
    SideLock,
}

impl PresetId {
    /// Same string serde uses; also the column name in `convar_catalog.csv`.
    pub fn key(self) -> &'static str {
        match self {
            PresetId::Vanilla => "vanilla",
            PresetId::Sqooky => "sqooky",
            PresetId::SqookyTest => "sqooky_test",
            PresetId::KaizMinspec => "kaiz_minspec",
            PresetId::KaizExtremelow => "kaiz_extremelow",
            PresetId::BootMaxfps => "boot_maxfps",
            PresetId::OptilockRecommended => "optilock_recommended",
            PresetId::OptilockPotato => "optilock_potato",
            PresetId::SideLock => "sidelock",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PresetInfo {
    pub id: PresetId,
    pub label: &'static str,
    pub author: &'static str,
    pub source: Source,
}

#[derive(Clone, Debug)]
pub enum Source {
    Pinned(Pinned),
    Remote(Remote),
}

/// A GPL preset embedded in the binary.
#[derive(Clone, Debug)]
pub struct Pinned {
    pub upstream_gameinfo: &'static str,
    pub upstream_video: Option<&'static str>,
    pub gameinfo: &'static str,
    pub video: Option<&'static str>,
}

/// A preset we may link to but not ship. Only its gameinfo.gi ConVars block is used.
#[derive(Clone, Copy, Debug)]
pub struct Remote {
    pub id: PresetId,
    /// The mod page people see.
    pub page: &'static str,
    /// GameBanana's ProfilePage API for the same mod; its `_aFiles` lists the downloads.
    pub api: &'static str,
    pub licence: &'static str,
    /// sha256 of the gameinfo.gi DeadTune was checked against.
    pub sha256: &'static str,
}

impl PresetInfo {
    pub fn pinned(&self) -> Option<&Pinned> {
        match &self.source {
            Source::Pinned(p) => Some(p),
            Source::Remote(_) => None,
        }
    }

    pub fn remote(&self) -> Option<&Remote> {
        match &self.source {
            Source::Remote(r) => Some(r),
            Source::Pinned(_) => None,
        }
    }

    /// Where the preset comes from, for credits.
    pub fn source_url(&self) -> &'static str {
        match &self.source {
            Source::Pinned(p) => p.upstream_gameinfo,
            Source::Remote(r) => r.page,
        }
    }
}

pub const CACHE_DIR: &str = "presets";

/// Downloaded and imported remote presets, one folder per preset. The GUI and the CLI both
/// pass their data dir, so an import in one shows up in the other.
pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(CACHE_DIR)
}

/// The stock ConVars block the denylist resets to.
pub fn vanilla_gameinfo() -> &'static str {
    info(PresetId::Vanilla)
        .pinned()
        .expect("vanilla is embedded")
        .gameinfo
}

/// The [`Remote`] entry of a remote preset; panics for an embedded one.
pub fn remote(id: PresetId) -> &'static Remote {
    info(id)
        .remote()
        .expect("remote() is only called for remote presets")
}

macro_rules! sqooky {
    ($file:literal, $url:literal) => {
        (
            concat!(
                "https://raw.githubusercontent.com/Sqooky/OptimizationLock/main/",
                $url
            ),
            include_str!(concat!(
                "../../../research/configs/OptimizationLock/",
                $file
            )),
        )
    };
}

macro_rules! optilock {
    ($file:literal, $url:literal) => {
        (
            concat!(
                "https://raw.githubusercontent.com/dacooderr/OptiLock/main/",
                $url
            ),
            include_str!(concat!("../../../research/configs/OptiLock/", $file)),
        )
    };
}

const fn preset(
    id: PresetId,
    label: &'static str,
    author: &'static str,
    gameinfo: (&'static str, &'static str),
    video: Option<(&'static str, &'static str)>,
) -> PresetInfo {
    PresetInfo {
        id,
        label,
        author,
        source: Source::Pinned(Pinned {
            upstream_gameinfo: gameinfo.0,
            gameinfo: gameinfo.1,
            upstream_video: match video {
                Some(v) => Some(v.0),
                None => None,
            },
            video: match video {
                Some(v) => Some(v.1),
                None => None,
            },
        }),
    }
}

static PRESETS: [PresetInfo; 9] = [
    preset(
        PresetId::Vanilla,
        "Vanilla",
        "Valve",
        sqooky!(
            "clean gameinfo.gi/gameinfo.gi",
            "clean%20gameinfo.gi/gameinfo.gi"
        ),
        None,
    ),
    preset(
        PresetId::Sqooky,
        "Sqooky",
        "Sqooky",
        sqooky!("Sqooky's .gi/gameinfo.gi", "Sqooky%27s%20.gi/gameinfo.gi"),
        None,
    ),
    preset(
        PresetId::SqookyTest,
        "Sqooky (testing)",
        "Sqooky",
        sqooky!("test_cfg/gameinfo.gi", "test_cfg/gameinfo.gi"),
        Some(sqooky!("test_cfg/video.txt", "test_cfg/video.txt")),
    ),
    preset(
        PresetId::KaizMinspec,
        "Kaizuchaneru minimum spec",
        "Kaizuchaneru",
        sqooky!(
            "kaizuchanerus minimum spec/gameinfo.gi",
            "kaizuchanerus%20minimum%20spec/gameinfo.gi"
        ),
        None,
    ),
    preset(
        PresetId::KaizExtremelow,
        "Kaizuchaneru extreme low",
        "Kaizuchaneru",
        sqooky!(
            "kaizuchanerus minimum spec/gameinfoextremelow.gi",
            "kaizuchanerus%20minimum%20spec/gameinfoextremelow.gi"
        ),
        None,
    ),
    preset(
        PresetId::BootMaxfps,
        "Boot max FPS",
        "Boot",
        sqooky!(
            "boot's maxium fps config/gameinfo.gi",
            "boot%27s%20maxium%20fps%20config/gameinfo.gi"
        ),
        None,
    ),
    preset(
        PresetId::OptilockRecommended,
        "OptiLock recommended",
        "dacooderr",
        optilock!(
            "OptiLock FPS Config (Recommended)/gameinfo.gi",
            "OptiLock%20FPS%20Config%20(Recommended)/gameinfo.gi"
        ),
        Some(optilock!(
            "OptiLock FPS Config (Recommended)/video.txt",
            "OptiLock%20FPS%20Config%20(Recommended)/video.txt"
        )),
    ),
    preset(
        PresetId::OptilockPotato,
        "OptiLock potato",
        "dacooderr",
        optilock!(
            "OptiLock Potato Config/gameinfo.gi",
            "OptiLock%20Potato%20Config/gameinfo.gi"
        ),
        Some(optilock!(
            "OptiLock Potato Config/video.txt",
            "OptiLock%20Potato%20Config/video.txt"
        )),
    ),
    PresetInfo {
        id: PresetId::SideLock,
        label: "SideLock",
        author: "hitmeupwhenyourelonely",
        source: Source::Remote(Remote {
            id: PresetId::SideLock,
            page: "https://gamebanana.com/mods/722944",
            api: "https://gamebanana.com/apiv11/Mod/722944/ProfilePage",
            licence: "CC BY-NC-ND 4.0",
            // gameinfo.gi in cfg.zip v1.0.0, gamebanana.com/dl/1833750, added 2026-10-01.
            sha256: "a895885ad0fb71d203b8b2506c32ffd90be2adb4f04c1c4f78b9027ea05e057c",
        }),
    },
];

pub fn all() -> &'static [PresetInfo] {
    &PRESETS
}

pub fn info(id: PresetId) -> &'static PresetInfo {
    PRESETS
        .iter()
        .find(|p| p.id == id)
        .expect("every PresetId has an entry in PRESETS")
}

#[cfg(feature = "fetch")]
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("download: {0}")]
    Http(#[from] ureq::Error),
    #[error("cache: {0}")]
    Io(#[from] std::io::Error),
}

/// Downloads an embedded preset's current upstream gameinfo.gi and stores it as
/// `<cache_dir>/<key>/gameinfo.gi`.
#[cfg(feature = "fetch")]
pub fn fetch_latest(pinned: &Pinned, cache_dir: &Path, id: PresetId) -> Result<String, FetchError> {
    fetch_into(
        pinned.upstream_gameinfo,
        &cache_dir.join(id.key()).join("gameinfo.gi"),
    )
}

/// Same as [`fetch_latest`] for video.txt; `None` for presets that ship without one.
#[cfg(feature = "fetch")]
pub fn fetch_latest_video(
    pinned: &Pinned,
    cache_dir: &Path,
    id: PresetId,
) -> Result<Option<String>, FetchError> {
    pinned
        .upstream_video
        .map(|url| fetch_into(url, &cache_dir.join(id.key()).join("video.txt")))
        .transpose()
}

#[cfg(feature = "fetch")]
fn fetch_into(url: &str, path: &Path) -> Result<String, FetchError> {
    let text = ureq::get(url).call()?.body_mut().read_to_string()?;
    let dir = path.parent().expect("cache path has a parent");
    std::fs::create_dir_all(dir)?;
    // Write-then-rename so a reader never sees a half-written cache file.
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &text)?;
    std::fs::rename(&tmp, path)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_IDS: [PresetId; 9] = [
        PresetId::Vanilla,
        PresetId::Sqooky,
        PresetId::SqookyTest,
        PresetId::KaizMinspec,
        PresetId::KaizExtremelow,
        PresetId::BootMaxfps,
        PresetId::OptilockRecommended,
        PresetId::OptilockPotato,
        PresetId::SideLock,
    ];

    fn pinned() -> impl Iterator<Item = (PresetId, &'static Pinned)> {
        all().iter().filter_map(|p| p.pinned().map(|x| (p.id, x)))
    }

    #[test]
    fn every_id_has_exactly_one_info() {
        assert_eq!(all().iter().map(|p| p.id).collect::<Vec<_>>(), ALL_IDS);
        for id in ALL_IDS {
            assert_eq!(info(id).id, id);
        }
    }

    #[test]
    fn pinned_gameinfo_has_a_convars_block() {
        for (id, p) in pinned() {
            assert!(
                p.gameinfo.contains("ConVars"),
                "{id:?} pinned gameinfo lacks ConVars"
            );
        }
    }

    #[test]
    fn video_is_pinned_exactly_where_upstream_has_one() {
        for (id, p) in pinned() {
            assert_eq!(p.video.is_some(), p.upstream_video.is_some(), "{id:?}");
            if let Some(v) = p.video {
                assert!(v.contains("setting."), "{id:?} video.txt has no settings");
            }
        }
        let video = |id| info(id).pinned().unwrap().video;
        assert!(video(PresetId::OptilockPotato).is_some());
        assert!(video(PresetId::KaizMinspec).is_none());
    }

    #[test]
    fn extremelow_is_a_different_file_from_minspec() {
        let gameinfo = |id| info(id).pinned().unwrap().gameinfo;
        assert_ne!(
            gameinfo(PresetId::KaizMinspec),
            gameinfo(PresetId::KaizExtremelow)
        );
        assert!(
            info(PresetId::KaizExtremelow)
                .source_url()
                .ends_with("gameinfoextremelow.gi")
        );
    }

    #[test]
    fn upstream_urls_are_raw_github_without_unescaped_spaces() {
        for (_, p) in pinned() {
            for url in std::iter::once(p.upstream_gameinfo).chain(p.upstream_video) {
                assert!(
                    url.starts_with("https://raw.githubusercontent.com/"),
                    "{url}"
                );
                assert!(!url.contains(' ') && !url.contains('\''), "{url}");
            }
        }
    }

    #[test]
    fn sidelock_is_remote_with_credit_licence_and_a_pinned_hash() {
        let info = info(PresetId::SideLock);
        assert!(info.pinned().is_none(), "CC BY-NC-ND: never embedded");
        assert_eq!(info.author, "hitmeupwhenyourelonely");
        let r = remote(PresetId::SideLock);
        assert_eq!(r.id, PresetId::SideLock);
        assert_eq!(r.licence, "CC BY-NC-ND 4.0");
        assert_eq!(info.source_url(), "https://gamebanana.com/mods/722944");
        assert!(
            r.api
                .starts_with("https://gamebanana.com/apiv11/Mod/722944/")
        );
        assert_eq!(r.sha256.len(), 64);
        assert!(r.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[cfg(feature = "fetch")]
    #[test]
    #[ignore = "network"]
    fn fetch_latest_downloads_and_caches_every_preset() {
        let dir = tempfile::tempdir().unwrap();
        for (id, p) in pinned() {
            let text = fetch_latest(p, dir.path(), id).unwrap();
            assert!(text.contains("ConVars"), "{id:?}");
            let cached =
                std::fs::read_to_string(dir.path().join(id.key()).join("gameinfo.gi")).unwrap();
            assert_eq!(cached, text);
            assert_eq!(
                fetch_latest_video(p, dir.path(), id).unwrap().is_some(),
                p.upstream_video.is_some()
            );
        }
    }

    #[test]
    fn key_matches_serde_name() {
        for id in ALL_IDS {
            let value =
                toml::Value::try_from(std::collections::BTreeMap::from([("k", id)])).unwrap();
            assert_eq!(value["k"].as_str(), Some(id.key()));
        }
    }
}
