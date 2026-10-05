//! `dt_core::practice` against Valve's live gameinfo.gi and Sqooky's older clean copy.

use std::collections::BTreeSet;

use dt_core::gi::GiError;
use dt_core::gi_sections::section_values;
use dt_core::practice::{self, Group, KEYS, PracticeMode, SECTION, STOCK};

const CLEAN: &str =
    include_str!("../../../research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi");

/// What SideLock's SceneSystem looks like: the same keys, its own order and spacing.
const SIDELOCK_SCENE: &str = "\tSceneSystem\n\t{\n\t\tGpuLightBinner 1\n\t\tFogCachedShadowAtlasWidth 0\n\t\tFogCachedShadowAtlasHeight 0\n\t\tFogCachedShadowTileSize 0\n\t\tCSMCascadeResolution 0\n\t\tDefaultShadowTextureWidth 0\n\t\tDefaultShadowTextureHeight 0\n\t\tDynamicShadowResolution 0\n\n\t\tLayerBatchThresholdFullsort 20\n\t\tNonTexturedGradientFog\t\t0\n\t\t// Temp till I can add support in citadel shaders\n\t\tDisableLateAllocatedTransformBuffer 1\n\t\tMinimumLateAllocatedVertexCacheBufferSizeMB 64\n\t\tCubemapFog 0\n\t\tVolumetricFog 0\n\t\tTonemapping 0\n\t}\n";

fn to_crlf(text: &str) -> String {
    text.replace('\n', "\r\n")
}

fn differing_lines<'a>(a: &'a str, b: &'a str) -> Vec<(&'a str, &'a str)> {
    a.lines().zip(b.lines()).filter(|(x, y)| x != y).collect()
}

fn is_batching_key(line: &str) -> bool {
    let key = line.split_whitespace().next().unwrap_or("");
    KEYS.iter()
        .any(|k| k.group == Group::Batching && k.key == key)
}

fn managed_keys(pick: impl Fn(&practice::ManagedKey) -> bool) -> BTreeSet<&'static str> {
    KEYS.iter().filter(|k| pick(k)).map(|k| k.key).collect()
}

/// SceneSystem keys whose value differs between two files.
fn touched_keys(before: &str, after: &str) -> BTreeSet<&'static str> {
    let a = section_values(before, SECTION).unwrap().unwrap();
    let b = section_values(after, SECTION).unwrap().unwrap();
    a.keys()
        .chain(b.keys())
        .filter(|k| a.get(*k) != b.get(*k))
        .map(|k| {
            KEYS.iter()
                .find(|m| m.key == k)
                .map_or("<other>", |m| m.key)
        })
        .collect()
}

fn outside_scene(text: &str) -> String {
    let start = text.find("\tSceneSystem").unwrap();
    let end = text.find("\tNavSystem").unwrap();
    format!("{}{}", &text[..start], &text[end..])
}

fn mode(shadows: bool, fog: bool, batching: bool) -> PracticeMode {
    PracticeMode {
        shadows,
        fog,
        batching,
    }
}

fn all_modes() -> Vec<PracticeMode> {
    (0..8)
        .map(|bits| mode(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0))
        .collect()
}

#[test]
fn stock_values_come_from_the_live_fixture() {
    let scene = section_values(STOCK, SECTION).unwrap().unwrap();
    for k in &KEYS {
        assert_eq!(
            scene.get(k.key).map(String::as_str),
            k.stock,
            "{} in the 2026-09-29 fixture",
            k.key
        );
        assert_ne!(
            k.practice,
            k.stock.unwrap_or(""),
            "{} would be a no-op",
            k.key
        );
    }
    assert_eq!(KEYS.iter().filter(|k| k.group == Group::Shadows).count(), 7);
    assert_eq!(KEYS.iter().filter(|k| k.group == Group::Fog).count(), 3);
    assert_eq!(
        KEYS.iter().filter(|k| k.group == Group::Batching).count(),
        3
    );
}

#[test]
fn off_leaves_both_fixtures_byte_identical() {
    for text in [STOCK, CLEAN] {
        assert_eq!(
            practice::apply(text, PracticeMode::default()).unwrap(),
            text
        );
        let crlf = to_crlf(text);
        assert_eq!(
            practice::apply(&crlf, PracticeMode::default()).unwrap(),
            crlf
        );
    }
}

#[test]
fn all_on_changes_only_the_managed_lines() {
    let on = practice::apply(STOCK, PracticeMode::ALL_ON).unwrap();
    assert_eq!(touched_keys(STOCK, &on), managed_keys(|_| true));
    assert_eq!(on.lines().count(), STOCK.lines().count() + 3);
    let without_inserts: String = on
        .lines()
        .filter(|l| !is_batching_key(l))
        .map(|l| format!("{l}\n"))
        .collect();
    let changed = differing_lines(STOCK, &without_inserts);
    assert_eq!(changed.len(), 10, "{changed:#?}");
    for (before, after) in &changed {
        let key = before.split_whitespace().next().unwrap();
        let k = KEYS.iter().find(|k| k.key == key).expect(key);
        let stock = k.stock.unwrap();
        assert!(before.ends_with(stock), "{before:?}");
        assert_eq!(
            *after,
            format!("{}{}", &before[..before.len() - stock.len()], k.practice),
            "only the value changed"
        );
        assert!(after.starts_with("\t\t"), "indentation kept: {after:?}");
    }
    assert!(
        on.contains(
            "\t\tComputeShaderSkinning 1\n\t\tDisableLateAllocatedTransformBuffer 1\n\t\tLayerBatchThresholdFullsort 20\n\t\tMinimumLateAllocatedVertexCacheBufferSizeMB 64\n\t}\n\n\tNavSystem"
        ),
        "inserted before the closing brace, in key order: {on}"
    );
    assert!(
        on.contains("\t\tNonTexturedGradientFog\t\t0\n"),
        "tab padding kept"
    );
    assert_eq!(
        outside_scene(&on),
        outside_scene(STOCK),
        "other sections untouched"
    );
    assert_eq!(practice::detect(&on).unwrap(), PracticeMode::ALL_ON);
}

#[test]
fn each_group_touches_only_its_own_keys() {
    for group in Group::ALL {
        let mut m = PracticeMode::default();
        m.set(group, true);
        let out = practice::apply(STOCK, m).unwrap();
        assert_eq!(
            touched_keys(STOCK, &out),
            managed_keys(|k| k.group == group),
            "{group:?}"
        );
        assert_eq!(outside_scene(&out), outside_scene(STOCK));
        let added = out.lines().count() - STOCK.lines().count();
        assert_eq!(added, if group == Group::Batching { 3 } else { 0 });
        assert_eq!(practice::detect(&out).unwrap(), m);
    }
}

#[test]
fn reapplying_is_idempotent_and_off_restores_stock_byte_for_byte() {
    for m in all_modes() {
        for text in [STOCK, CLEAN] {
            let once = practice::apply(text, m).unwrap();
            let twice = practice::apply(&once, m).unwrap();
            assert_eq!(once, twice, "{m:?}");
            assert_eq!(practice::detect(&once).unwrap(), m);
            assert_eq!(practice::restore_stock(&once).unwrap(), text, "{m:?}");
        }
    }
}

#[test]
fn crlf_files_stay_crlf() {
    let crlf = to_crlf(STOCK);
    let on = practice::apply(&crlf, PracticeMode::ALL_ON).unwrap();
    assert_eq!(
        on,
        to_crlf(&practice::apply(STOCK, PracticeMode::ALL_ON).unwrap())
    );
    assert!(!on.replace("\r\n", "").contains('\n'));
    assert_eq!(practice::restore_stock(&on).unwrap(), crlf);
}

#[test]
fn quoted_values_in_the_older_clean_copy_keep_their_quotes() {
    let on = practice::apply(CLEAN, PracticeMode::ALL_ON).unwrap();
    assert!(
        on.contains("CSMCascadeResolution           \"0\"\n"),
        "{on}"
    );
    assert!(on.contains("LayerBatchThresholdFullsort \"20\"\n"), "{on}");
    assert_eq!(practice::detect(&on).unwrap(), PracticeMode::ALL_ON);
    assert_eq!(practice::restore_stock(&on).unwrap(), CLEAN);
}

#[test]
fn sidelocks_own_layout_reads_as_all_on_and_restores_to_stock_values() {
    let start = STOCK.find("\tSceneSystem").unwrap();
    let end = STOCK.find("\tNavSystem").unwrap();
    let sidelock = format!("{}{}\n{}", &STOCK[..start], SIDELOCK_SCENE, &STOCK[end..]);
    assert_eq!(practice::detect(&sidelock).unwrap(), PracticeMode::ALL_ON);
    let restored = practice::restore_stock(&sidelock).unwrap();
    assert_eq!(
        practice::detect(&restored).unwrap(),
        PracticeMode::default()
    );
    assert!(restored.contains("\t\tCSMCascadeResolution 2048\n"));
    assert!(!restored.contains("LayerBatchThresholdFullsort"));
    assert!(
        restored
            .contains("\t\t// Temp till I can add support in citadel shaders\n\t\tCubemapFog 1\n"),
        "SideLock's comment stays, its extra keys go: {restored}"
    );
    assert!(
        practice::matchmaking_drift(&restored)
            .unwrap()
            .iter()
            .all(|d| !d.managed()),
        "no managed drift after restore"
    );
}

#[test]
fn a_file_without_the_section_is_left_alone_when_off_and_refused_when_on() {
    let start = STOCK.find("\tSceneSystem").unwrap();
    let end = STOCK.find("\tNavSystem").unwrap();
    let without = format!("{}{}", &STOCK[..start], &STOCK[end..]);
    assert_eq!(
        practice::apply(&without, PracticeMode::default()).unwrap(),
        without
    );
    assert_eq!(
        practice::apply(&without, mode(true, false, false)),
        Err(GiError::NoSection(SECTION.into()))
    );
    assert_eq!(practice::detect(&without).unwrap(), PracticeMode::default());
}

#[test]
fn drift_separates_practice_keys_from_other_tools_leftovers() {
    assert_eq!(practice::matchmaking_drift(STOCK).unwrap(), Vec::new());

    let on = practice::apply(STOCK, mode(false, true, false)).unwrap();
    let drift = practice::matchmaking_drift(&on).unwrap();
    let keys: Vec<&str> = drift.iter().map(|d| d.key.as_str()).collect();
    assert_eq!(
        keys,
        ["CubemapFog", "NonTexturedGradientFog", "VolumetricFog"]
    );
    assert!(drift.iter().all(|d| d.managed() && d.section == SECTION));
    assert_eq!(drift[0].stock.as_deref(), Some("1"));
    assert_eq!(drift[0].live.as_deref(), Some("0"));

    let leftovers = on
        .replacen(
            "\t\tVulkanMutableSwapchain 1\n",
            "\t\tSwapChainSampleableDepth 1\n\t\tVulkanMutableSwapchain 1\n\t\t\"VulkanOnly_Linux\"\t\"1\"\n",
            1,
        )
        .lines()
        .map(|l| {
            if l.contains("\"LowLatency\"") {
                format!("{}\n", l.replace("\"1\"", "\"0\""))
            } else {
                format!("{l}\n")
            }
        })
        .collect::<String>();
    let drift = practice::matchmaking_drift(&leftovers).unwrap();
    let foreign: Vec<(&str, &str, Option<&str>, Option<&str>)> = drift
        .iter()
        .filter(|d| !d.managed())
        .map(|d| {
            (
                d.section,
                d.key.as_str(),
                d.stock.as_deref(),
                d.live.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        foreign,
        [
            ("RenderSystem", "LowLatency", Some("1"), Some("0")),
            ("RenderSystem", "SwapChainSampleableDepth", None, Some("1")),
            ("RenderSystem", "VulkanOnly_Linux", None, Some("1")),
        ]
    );
    assert_eq!(drift.iter().filter(|d| d.managed()).count(), 3);
}

#[test]
fn mode_helpers() {
    assert!(PracticeMode::default().is_off());
    assert!(!PracticeMode::ALL_ON.is_off());
    assert_eq!(
        mode(true, false, true).groups_on(),
        [Group::Shadows, Group::Batching]
    );
    let text = toml::to_string(&mode(false, true, false)).unwrap();
    assert_eq!(text, "shadows = false\nfog = true\nbatching = false\n");
    assert_eq!(
        toml::from_str::<PracticeMode>("fog = true\n").unwrap(),
        mode(false, true, false)
    );
}
