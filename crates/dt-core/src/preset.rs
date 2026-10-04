//! Known community presets. Pinned copies are embedded; the `fetch` feature refreshes
//! them from upstream GitHub into the cache dir.

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
        }
    }
}

#[derive(Clone, Debug)]
pub struct PresetInfo {
    pub id: PresetId,
    pub label: &'static str,
    pub author: &'static str,
    pub upstream_gameinfo: &'static str,
    pub upstream_video: Option<&'static str>,
    pub pinned_gameinfo: &'static str,
    pub pinned_video: Option<&'static str>,
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
        upstream_gameinfo: gameinfo.0,
        pinned_gameinfo: gameinfo.1,
        upstream_video: match video {
            Some(v) => Some(v.0),
            None => None,
        },
        pinned_video: match video {
            Some(v) => Some(v.1),
            None => None,
        },
    }
}

static PRESETS: [PresetInfo; 8] = [
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

/// Downloads the preset's current upstream gameinfo.gi and stores it as
/// `<cache_dir>/<key>/gameinfo.gi`.
#[cfg(feature = "fetch")]
pub fn fetch_latest(id: PresetId, cache_dir: &std::path::Path) -> Result<String, FetchError> {
    fetch_into(
        info(id).upstream_gameinfo,
        &cache_dir.join(id.key()).join("gameinfo.gi"),
    )
}

/// Same as [`fetch_latest`] for video.txt; `None` for presets that ship without one.
#[cfg(feature = "fetch")]
pub fn fetch_latest_video(
    id: PresetId,
    cache_dir: &std::path::Path,
) -> Result<Option<String>, FetchError> {
    info(id)
        .upstream_video
        .map(|url| fetch_into(url, &cache_dir.join(id.key()).join("video.txt")))
        .transpose()
}

#[cfg(feature = "fetch")]
fn fetch_into(url: &str, path: &std::path::Path) -> Result<String, FetchError> {
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

    const ALL_IDS: [PresetId; 8] = [
        PresetId::Vanilla,
        PresetId::Sqooky,
        PresetId::SqookyTest,
        PresetId::KaizMinspec,
        PresetId::KaizExtremelow,
        PresetId::BootMaxfps,
        PresetId::OptilockRecommended,
        PresetId::OptilockPotato,
    ];

    #[test]
    fn every_id_has_exactly_one_info() {
        assert_eq!(all().iter().map(|p| p.id).collect::<Vec<_>>(), ALL_IDS);
        for id in ALL_IDS {
            assert_eq!(info(id).id, id);
        }
    }

    #[test]
    fn pinned_gameinfo_has_a_convars_block() {
        for p in all() {
            assert!(
                p.pinned_gameinfo.contains("ConVars"),
                "{:?} pinned gameinfo lacks ConVars",
                p.id
            );
        }
    }

    #[test]
    fn video_is_pinned_exactly_where_upstream_has_one() {
        for p in all() {
            assert_eq!(
                p.pinned_video.is_some(),
                p.upstream_video.is_some(),
                "{:?}",
                p.id
            );
            if let Some(v) = p.pinned_video {
                assert!(
                    v.contains("setting."),
                    "{:?} video.txt has no settings",
                    p.id
                );
            }
        }
        assert!(info(PresetId::OptilockPotato).pinned_video.is_some());
        assert!(info(PresetId::KaizMinspec).pinned_video.is_none());
    }

    #[test]
    fn extremelow_is_a_different_file_from_minspec() {
        assert_ne!(
            info(PresetId::KaizMinspec).pinned_gameinfo,
            info(PresetId::KaizExtremelow).pinned_gameinfo
        );
        assert!(
            info(PresetId::KaizExtremelow)
                .upstream_gameinfo
                .ends_with("gameinfoextremelow.gi")
        );
    }

    #[test]
    fn upstream_urls_are_raw_github_without_unescaped_spaces() {
        for p in all() {
            for url in std::iter::once(p.upstream_gameinfo).chain(p.upstream_video) {
                assert!(
                    url.starts_with("https://raw.githubusercontent.com/"),
                    "{url}"
                );
                assert!(!url.contains(' ') && !url.contains('\''), "{url}");
            }
        }
    }

    #[cfg(feature = "fetch")]
    #[test]
    #[ignore = "network"]
    fn fetch_latest_downloads_and_caches_every_preset() {
        let dir = tempfile::tempdir().unwrap();
        for p in all() {
            let text = fetch_latest(p.id, dir.path()).unwrap();
            assert!(text.contains("ConVars"), "{:?}", p.id);
            let cached =
                std::fs::read_to_string(dir.path().join(p.id.key()).join("gameinfo.gi")).unwrap();
            assert_eq!(cached, text);
            assert_eq!(
                fetch_latest_video(p.id, dir.path()).unwrap().is_some(),
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
