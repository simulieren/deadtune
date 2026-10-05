use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use dt_core::addons::verify;
use dt_core::hud::apples_tunnels::{self, MINIMAP_LAYOUT};
use dt_core::hud::icons;
use dt_core::hud::ingame::{self, IngameSettings, SETTINGS_LAYOUT};
use dt_core::hud::inject;
use dt_core::hud::install::{self, ADDON_FILE, GAME_PAK, HudAction, HudError, InstalledState};
use dt_core::hud::resource::{Resource, style_text};
use dt_core::hud::topbar::{OWN_SCRIPT, OWN_STYLE, TOP_BAR_LAYOUT, TOP_BAR_STYLE};
use dt_core::hud::vpk::{self, VpkDir, VpkError};
use dt_core::hud::{
    Color, ElementEdit, ElementId, HudLayout, HudPatch, IconId, TopBarStyle, layout,
};
use dt_core::locate::{self, GamePaths};
use dt_core::texture::encode::Fit;
use dt_core::texture::png::{self, RgbaImage};
use dt_core::texture::vtex::{Flags, Vtex};
use dt_core::usercfg;

const HUD: &str = "panorama/styles/hud.vcss_c";
const MINIMAP: &str = "panorama/styles/hud_minimap.vcss_c";
const CSS: &str = "#TopBar{opacity:0.5;}";
const ICON: &str = "panorama/images/hud/minimap/objective_icon_psd.vtex_c";

fn repo_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

struct Fake {
    _tmp: tempfile::TempDir,
    paths: GamePaths,
    state: PathBuf,
}

impl Fake {
    fn new() -> Fake {
        Fake::with_pak(true)
    }

    fn game(&self) -> VpkDir {
        VpkDir::open(&self.paths.citadel_dir.join(GAME_PAK)).unwrap()
    }

    fn with_pak(pak: bool) -> Fake {
        Fake::build(pak.then(|| inject::compiled_layout(&ingame::stand_in_layout())))
    }

    /// A game pak whose settings menu is `settings`; `None` means no game pak at all.
    fn build(settings: Option<Vec<u8>>) -> Fake {
        let tmp = tempfile::tempdir().unwrap();
        let steamapps = tmp.path().join("steamapps");
        let root = steamapps.join("common/Deadlock");
        let citadel = root.join("game/citadel");
        fs::create_dir_all(citadel.join("cfg")).unwrap();
        fs::copy(
            repo_file("research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi"),
            citadel.join("gameinfo.gi"),
        )
        .unwrap();
        fs::copy(
            repo_file("research/configs/OptimizationLock/test_cfg/video.txt"),
            citadel.join("cfg/video.txt"),
        )
        .unwrap();
        write_manifest(&steamapps.join("appmanifest_1422450.acf"), "20261004");
        if let Some(settings) = settings {
            let mut files = BTreeMap::new();
            files.insert(SETTINGS_LAYOUT.to_string(), settings);
            files.insert(HUD.to_string(), vanilla_hud());
            // Any compiled stylesheet works as the minimap and top bar templates.
            files.insert(MINIMAP.to_string(), vanilla_hud());
            files.insert(TOP_BAR_STYLE.to_string(), vanilla_hud());
            files.insert(TOP_BAR_LAYOUT.to_string(), vanilla_top_bar());
            files.insert(MINIMAP_LAYOUT.to_string(), vanilla_minimap_layout());
            files.insert("scripts/unrelated.txt".to_string(), b"unrelated".to_vec());
            files.insert(ICON.to_string(), game_icon());
            fs::write(citadel.join(GAME_PAK), vpk::write(&files)).unwrap();
        }
        let paths = locate::from_game_root(&root).unwrap();
        assert!(
            paths.appmanifest.is_some(),
            "fake install exposes its appmanifest"
        );
        let state = tmp.path().join("state");
        Fake {
            _tmp: tmp,
            paths,
            state,
        }
    }

    fn addon(&self) -> PathBuf {
        install::addons_dir(&self.paths).join(ADDON_FILE)
    }

    fn record(&self) -> PathBuf {
        self.state.join(install::RECORD_FILE)
    }

    /// Rewrites the game's pak01 with `path` holding `bytes`, as a game update would.
    fn update_game_file(&self, path: &str, bytes: Vec<u8>) {
        let pak_path = self.paths.citadel_dir.join(GAME_PAK);
        let game = VpkDir::open(&pak_path).unwrap();
        let mut files: BTreeMap<String, Vec<u8>> = game
            .entries
            .keys()
            .map(|p| (p.clone(), game.read(p).unwrap()))
            .collect();
        files.insert(path.to_string(), bytes);
        fs::write(pak_path, vpk::write(&files)).unwrap();
    }

    fn install(&self, patch: HudPatch) {
        let plan = install::plan_patch(&self.paths, patch, &self.state).unwrap();
        install::execute(&plan, &self.paths, &self.state).unwrap();
    }
}

fn write_manifest(path: &Path, build: &str) {
    fs::write(
        path,
        format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"1422450\"\n\t\"name\"\t\t\"Deadlock\"\n\t\"buildid\"\t\t\"{build}\"\n\t\"installdir\"\t\t\"Deadlock\"\n}}\n"
        ),
    )
    .unwrap();
}

/// A real single-mip NO_LOD game texture (ATI1N, 1024x1024).
fn game_icon() -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/texture/emissive_1024_ati1n_nolod_1mip.vtex_c"),
    )
    .unwrap()
}

/// The Vindicta scope texture from research: the game's own 1080x1080 BGRA8888 header.
fn vindicta_texture() -> Vec<u8> {
    VpkDir::open(&repo_file(
        "research/configs/OptimizationLock/Various Addons Relating to Performance/Vindicta Scope Downscale/pak89_dir.vpk",
    ))
    .unwrap()
    .read("panorama/images/hud/crosshair/scope_common_psd.vtex_c")
    .unwrap()
}

fn test_png(width: u32, height: u32) -> Vec<u8> {
    let pixels = [255, 0, 255, 255].repeat(width as usize * height as usize);
    png::write(&RgbaImage::new(width, height, pixels).unwrap()).unwrap()
}

fn vanilla_hud() -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/hud_vanilla.vcss_c"))
        .unwrap()
}

fn vanilla_top_bar() -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/top_bar_vanilla.vxml_c"),
    )
    .unwrap()
}

fn vanilla_minimap_layout() -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/hud_minimap_vanilla.vxml_c"),
    )
    .unwrap()
}

#[test]
fn apples_and_tunnels_rebuild_the_minimap_layout_beside_the_top_bar() {
    let fake = Fake::new();
    let mut hud = HudLayout::default();
    hud.apples_tunnels.apples.on = true;
    hud.apples_tunnels.tunnels.on = true;
    hud.apples_tunnels.clear_switching = true;
    hud.top_bar.spawn_timers = true;
    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    install::execute(&plan, &fake.paths, &fake.state).unwrap();

    let addon = VpkDir::open(&fake.addon()).unwrap();
    let mut entries: Vec<&str> = addon.entries.keys().map(String::as_str).collect();
    entries.sort_unstable();
    assert_eq!(
        entries,
        [
            TOP_BAR_LAYOUT,
            MINIMAP_LAYOUT,
            apples_tunnels::OWN_SCRIPT,
            OWN_SCRIPT,
            apples_tunnels::OWN_STYLE,
            OWN_STYLE,
            MINIMAP,
        ]
    );
    let original = inject::layout_text(&vanilla_minimap_layout()).unwrap();
    let rebuilt = inject::layout_text(&addon.read(MINIMAP_LAYOUT).unwrap()).unwrap();
    assert!(rebuilt.starts_with("<!-- Rebuilt by DeadTune from the game's own panorama/layout/hud_minimap.vxml_c; adds s2r://panorama/styles/deadtune/apples_tunnels.vcss_c, s2r://panorama/scripts/deadtune/apples_tunnels.vjs_c -->\n<root>\n"), "{rebuilt}");
    assert!(inject::extends(&rebuilt, &original));
    assert_eq!(rebuilt.lines().count(), original.lines().count() + 5);

    let script = Resource::parse(&addon.read(apples_tunnels::OWN_SCRIPT).unwrap()).unwrap();
    assert_eq!(script.type_version, 4);
    assert!(
        script.blocks[0]
            .data
            .starts_with(b"var DT_MAP = {\"apples\":[[0.76464844,")
    );
    let sheet = Resource::parse(&addon.read(apples_tunnels::OWN_STYLE).unwrap()).unwrap();
    assert!(style_text(&sheet).unwrap().starts_with("#DtApples{"));
    let minimap = Resource::parse(&addon.read(MINIMAP).unwrap()).unwrap();
    assert!(
        style_text(&minimap)
            .unwrap()
            .ends_with("backgroundImage3{opacity:1;brightness:1.15;}")
    );

    let expect = verify::expect_for_hud(&fake.game(), &addon);
    let verified = verify::verify(&addon, &expect);
    assert!(verified.is_ok(), "{verified}");
}

fn patch(css: &str) -> HudPatch {
    HudPatch {
        styles: BTreeMap::from([(HUD.to_string(), css.to_string())]),
        ..HudPatch::default()
    }
}

#[test]
fn top_bar_extras_rebuild_the_layout_and_add_our_files() {
    let fake = Fake::new();
    let hud = HudLayout {
        top_bar: TopBarStyle {
            missing_opacity_pct: 20,
            spawn_timers: true,
            urn_lead: true,
            ..TopBarStyle::default()
        },
        ..HudLayout::default()
    };
    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    assert!(matches!(plan.action, HudAction::Write(_)));
    install::execute(&plan, &fake.paths, &fake.state).unwrap();

    let addon = VpkDir::open(&fake.addon()).unwrap();
    let mut entries: Vec<&str> = addon.entries.keys().map(String::as_str).collect();
    entries.sort_unstable();
    assert_eq!(
        entries,
        [TOP_BAR_LAYOUT, OWN_SCRIPT, TOP_BAR_STYLE, OWN_STYLE]
    );
    let res = Resource::parse(&addon.read(TOP_BAR_STYLE).unwrap()).unwrap();
    assert!(
        style_text(&res)
            .unwrap()
            .ends_with("#HeroContents{opacity:0.2;}")
    );

    let original = inject::layout_text(&vanilla_top_bar()).unwrap();
    let rebuilt = inject::layout_text(&addon.read(TOP_BAR_LAYOUT).unwrap()).unwrap();
    assert!(rebuilt.starts_with("<!-- Rebuilt by DeadTune from the game's own panorama/layout/citadel_hud_top_bar.vxml_c; adds s2r://panorama/styles/deadtune/top_bar.vcss_c, s2r://panorama/scripts/deadtune/top_bar.vjs_c -->\n<root>\n"), "{rebuilt}");
    assert!(inject::extends(&rebuilt, &original));
    assert_eq!(rebuilt.lines().count(), original.lines().count() + 5);
    assert!(rebuilt.contains("\t<scripts>\n\t\t<include src=\"s2r://panorama/scripts/deadtune/top_bar.vjs_c\" />\n\t</scripts>\n"));

    let script = Resource::parse(&addon.read(OWN_SCRIPT).unwrap()).unwrap();
    assert_eq!(script.type_version, 4);
    assert!(
        script.blocks[0].data.starts_with(
            b"var DT_TOP_BAR = { spawnTimers: true, urnLead: true, purchases: false };"
        )
    );
    let sheet = Resource::parse(&addon.read(OWN_STYLE).unwrap()).unwrap();
    assert!(style_text(&sheet).unwrap().starts_with("#DtSpawnTimers{"));

    let expect = verify::expect_for_hud(&fake.game(), &addon);
    assert_eq!(
        expect.checks.len(),
        2,
        "the two game files we patched: {expect:?}"
    );
    let verified = verify::verify(&addon, &expect);
    assert!(verified.is_ok(), "{verified}");
    match install::installed_state(&fake.paths, &fake.state).unwrap() {
        InstalledState::Current(r) => assert_eq!(
            r.patched,
            [TOP_BAR_STYLE, TOP_BAR_LAYOUT, OWN_SCRIPT, OWN_STYLE]
        ),
        other => panic!("expected Current, got {other:?}"),
    }

    let mut tampered = addon.read(TOP_BAR_LAYOUT).unwrap();
    let at = tampered.len() - 20;
    tampered[at] ^= 1;
    let mut files = BTreeMap::new();
    for path in addon.entries.keys() {
        files.insert(path.clone(), addon.read(path).unwrap());
    }
    files.insert(TOP_BAR_LAYOUT.to_string(), tampered);
    let bad = VpkDir::in_memory(vpk::write(&files)).unwrap();
    let verified = verify::verify(&bad, &verify::expect_for_hud(&fake.game(), &bad));
    assert!(
        verified
            .problems
            .iter()
            .any(|p| p.starts_with(TOP_BAR_LAYOUT) && p.contains("crc")),
        "{verified}"
    );
}

#[test]
fn write_installs_addon_and_record() {
    let fake = Fake::new();
    let plan = install::plan_patch(&fake.paths, patch(CSS), &fake.state).unwrap();
    assert!(matches!(plan.action, HudAction::Write(_)));
    assert!(plan.conflicts.is_empty());
    assert!(
        plan.needs_search_path,
        "clean gameinfo.gi has no citadel/addons line"
    );
    install::execute(&plan, &fake.paths, &fake.state).unwrap();

    let addon = VpkDir::open(&fake.addon()).unwrap();
    assert_eq!(addon.entries.keys().collect::<Vec<_>>(), [HUD]);
    let res = Resource::parse(&addon.read(HUD).unwrap()).unwrap();
    let vanilla = Resource::parse(&vanilla_hud()).unwrap();
    let text = style_text(&res).unwrap();
    assert!(text.ends_with(CSS));
    assert_eq!(text.len(), style_text(&vanilla).unwrap().len() + CSS.len());

    let record = fs::read_to_string(fake.record()).unwrap();
    assert!(record.contains("20261004"), "{record}");
    match install::installed_state(&fake.paths, &fake.state).unwrap() {
        InstalledState::Current(r) => {
            assert_eq!(r.patched, [HUD]);
            assert_eq!(r.build_id.as_deref(), Some("20261004"));
        }
        other => panic!("expected Current, got {other:?}"),
    }
}

#[test]
fn build_is_deterministic() {
    let fake = Fake::new();
    let pak = VpkDir::open(&fake.paths.citadel_dir.join(GAME_PAK)).unwrap();
    assert_eq!(
        install::build_addon(&pak, &patch(CSS), &fake.state).unwrap(),
        install::build_addon(&pak, &patch(CSS), &fake.state).unwrap()
    );
}

#[test]
fn same_patch_replans_to_nothing() {
    let fake = Fake::new();
    fake.install(patch(CSS));
    let plan = install::plan_patch(&fake.paths, patch(CSS), &fake.state).unwrap();
    assert_eq!(plan.action, HudAction::Nothing);
    assert!(
        plan.needs_search_path,
        "still unmounted until gameinfo.gi is fixed"
    );
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    assert!(matches!(
        install::installed_state(&fake.paths, &fake.state).unwrap(),
        InstalledState::Current(_)
    ));
}

#[test]
fn changed_patch_rewrites_our_file() {
    let fake = Fake::new();
    fake.install(patch(CSS));
    let plan = install::plan_patch(&fake.paths, patch("#Chat{opacity:0;}"), &fake.state).unwrap();
    assert!(matches!(plan.action, HudAction::Write(_)));
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    let addon = VpkDir::open(&fake.addon()).unwrap();
    let res = Resource::parse(&addon.read(HUD).unwrap()).unwrap();
    assert!(style_text(&res).unwrap().ends_with("#Chat{opacity:0;}"));
}

#[test]
fn buildid_bump_is_stale_and_rebuild_converges() {
    let fake = Fake::new();
    fake.install(patch(CSS));
    write_manifest(fake.paths.appmanifest.as_ref().unwrap(), "20261005");
    assert!(matches!(
        install::installed_state(&fake.paths, &fake.state).unwrap(),
        InstalledState::Stale(_)
    ));

    let plan = install::plan_patch(&fake.paths, patch(CSS), &fake.state).unwrap();
    assert!(matches!(plan.action, HudAction::Write(_)));
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    match install::installed_state(&fake.paths, &fake.state).unwrap() {
        InstalledState::Current(r) => assert_eq!(r.build_id.as_deref(), Some("20261005")),
        other => panic!("expected Current, got {other:?}"),
    }
}

#[test]
fn empty_patch_removes_addon_and_record_idempotently() {
    let fake = Fake::new();
    fake.install(patch(CSS));
    let plan = install::plan_patch(&fake.paths, HudPatch::default(), &fake.state).unwrap();
    assert_eq!(plan.action, HudAction::Remove);
    assert!(!plan.needs_search_path);
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    assert!(!fake.addon().exists());
    assert!(!fake.record().exists());
    install::execute(&plan, &fake.paths, &fake.state).unwrap();

    assert_eq!(
        install::installed_state(&fake.paths, &fake.state).unwrap(),
        InstalledState::None
    );
    let again = install::plan_patch(&fake.paths, patch(""), &fake.state).unwrap();
    assert_eq!(again.action, HudAction::Nothing);
}

#[test]
fn foreign_file_is_refused_and_untouched() {
    let fake = Fake::new();
    fs::create_dir_all(fake.addon().parent().unwrap()).unwrap();
    fs::write(fake.addon(), b"someone else's addon").unwrap();

    for p in [patch(CSS), HudPatch::default()] {
        let err = install::plan_patch(&fake.paths, p, &fake.state).unwrap_err();
        assert!(
            matches!(err, HudError::Foreign(ref path) if *path == fake.addon()),
            "{err}"
        );
    }
    assert_eq!(
        install::installed_state(&fake.paths, &fake.state).unwrap(),
        InstalledState::Foreign
    );
    assert_eq!(fs::read(fake.addon()).unwrap(), b"someone else's addon");
}

#[test]
fn foreign_file_appearing_after_plan_is_refused_by_execute() {
    let fake = Fake::new();
    let plan = install::plan_patch(&fake.paths, patch(CSS), &fake.state).unwrap();
    fs::create_dir_all(fake.addon().parent().unwrap()).unwrap();
    fs::write(fake.addon(), b"late arrival").unwrap();
    let err = install::execute(&plan, &fake.paths, &fake.state).unwrap_err();
    assert!(matches!(err, HudError::Foreign(_)), "{err}");
    assert_eq!(fs::read(fake.addon()).unwrap(), b"late arrival");
}

#[test]
fn other_addon_with_same_style_is_a_conflict() {
    let fake = Fake::new();
    let addons = install::addons_dir(&fake.paths);
    fs::create_dir_all(&addons).unwrap();
    let files = BTreeMap::from([(HUD.to_string(), vanilla_hud())]);
    fs::write(addons.join("pak03_dir.vpk"), vpk::write(&files)).unwrap();
    let unrelated = BTreeMap::from([("panorama/styles/other.vcss_c".to_string(), b"x".to_vec())]);
    fs::write(addons.join("pak04_dir.vpk"), vpk::write(&unrelated)).unwrap();
    fs::write(addons.join("pak05_dir.vpk"), b"not a vpk").unwrap();

    let plan = install::plan_patch(&fake.paths, patch(CSS), &fake.state).unwrap();
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].addon, addons.join("pak03_dir.vpk"));
    assert_eq!(plan.conflicts[0].paths, [HUD]);
}

#[test]
fn missing_game_pak_is_a_clear_error() {
    let fake = Fake::with_pak(false);
    let err = install::plan_patch(&fake.paths, patch(CSS), &fake.state).unwrap_err();
    assert!(
        matches!(err, HudError::MissingGamePak(ref p) if p.ends_with(GAME_PAK)),
        "{err}"
    );
    assert!(err.to_string().contains("pak01_dir.vpk"), "{err}");
}

#[test]
fn style_file_absent_from_game_pak_is_missing() {
    let fake = Fake::new();
    let p = patch(CSS);
    let mut files = p.styles;
    files.insert("panorama/styles/nope.vcss_c".to_string(), CSS.to_string());
    let patch = HudPatch {
        styles: files,
        ..HudPatch::default()
    };
    let err = install::plan_patch(&fake.paths, patch, &fake.state).unwrap_err();
    assert!(matches!(err, HudError::Vpk(VpkError::Missing(_))), "{err}");
}

#[test]
fn layout_compiles_installs_and_vanilla_removes() {
    let fake = Fake::new();
    let mut hud = HudLayout::default();
    hud.elements.insert(
        ElementId::Minimap,
        ElementEdit {
            scale_pct: 120,
            ..ElementEdit::default()
        },
    );
    hud.elements.insert(
        ElementId::TopBar,
        ElementEdit {
            offset_y: 30,
            ..ElementEdit::default()
        },
    );
    let css = layout::compile(&hud).unwrap().styles[HUD].clone();
    assert!(
        css.contains("#minimap_persp{") && css.contains("#TopBar{"),
        "{css}"
    );

    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    assert!(matches!(plan.action, HudAction::Write(_)));
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    let addon = VpkDir::open(&fake.addon()).unwrap();
    let res = Resource::parse(&addon.read(HUD).unwrap()).unwrap();
    assert!(style_text(&res).unwrap().ends_with(&css));
    assert_eq!(
        install::plan(&fake.paths, &hud, &fake.state)
            .unwrap()
            .action,
        HudAction::Nothing
    );

    let plan = install::plan(&fake.paths, &HudLayout::default(), &fake.state).unwrap();
    assert_eq!(plan.action, HudAction::Remove);
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    assert!(!fake.addon().exists());
}

fn ingame_rows() -> HudLayout {
    HudLayout {
        ingame: IngameSettings {
            wide_fov: true,
            performance: ["r_citadel_shadow_quality", "sc_clutter_enable"]
                .map(String::from)
                .into(),
        },
        ..HudLayout::default()
    }
}

/// Installs the in-game settings rows against the settings layout in the fake pak and
/// checks the rebuilt menu: the game's rows untouched and in order, ours in their places,
/// the pak verifying against the game file, and vanilla removing it all.
fn check_ingame_rows(fake: &Fake) -> String {
    let hud = ingame_rows();
    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    assert!(matches!(plan.action, HudAction::Write(_)));
    assert!(plan.conflicts.is_empty());
    install::execute(&plan, &fake.paths, &fake.state).unwrap();

    let addon = VpkDir::open(&fake.addon()).unwrap();
    let mut entries: Vec<&str> = addon.entries.keys().map(String::as_str).collect();
    entries.sort_unstable();
    assert_eq!(entries, [SETTINGS_LAYOUT, ingame::OWN_SCRIPT]);
    let game = VpkDir::open(&fake.paths.citadel_dir.join(GAME_PAK)).unwrap();
    let original = inject::layout_text(&game.read(SETTINGS_LAYOUT).unwrap()).unwrap();
    let rebuilt = inject::layout_text(&addon.read(SETTINGS_LAYOUT).unwrap()).unwrap();
    assert!(rebuilt.starts_with("<!-- Rebuilt by DeadTune from the game's own panorama/layout/popups/popup_settings.vxml_c; adds s2r://panorama/scripts/deadtune/ingame_settings.vjs_c -->\n<root>\n"), "{}", &rebuilt[..200]);
    assert!(inject::extends(&rebuilt, &original));
    assert_eq!(
        rebuilt.lines().count(),
        original.lines().count() + 1 + 3 + 3 + 2 + 6
    );
    let fov = rebuilt.find("id=\"CameraFOV\"").unwrap();
    let wide = rebuilt.find("id=\"DtWideFovRow\"").unwrap();
    let next_stock_row = rebuilt[fov..]
        .find("convar=\"citadel_camera_pitch_inverted\"")
        .unwrap()
        + fov;
    assert!(
        fov < wide && wide < next_stock_row,
        "the Wide FOV row sits right after the stock FOV row"
    );
    let group = rebuilt.find("id=\"citadel_settings_deadtune\"").unwrap();
    let advanced = rebuilt.find("id=\"citadel_settings_advanced\"").unwrap();
    assert!(advanced < group);
    assert!(rebuilt[group..].contains("convar=\"r_citadel_shadow_quality\""));
    assert!(rebuilt[group..].contains("convar=\"sc_clutter_enable\""));
    assert!(
        !rebuilt.contains("r_grass_quality"),
        "rows not picked stay out"
    );

    let script = Resource::parse(&addon.read(ingame::OWN_SCRIPT).unwrap()).unwrap();
    assert!(
        script.blocks[0]
            .data
            .starts_with(b"var DT_INGAME = { wideFov: true,")
    );
    let expect = verify::expect_for_hud(&fake.game(), &addon);
    assert_eq!(expect.checks.len(), 1, "{expect:?}");
    let verified = verify::verify(&addon, &expect);
    assert!(verified.is_ok(), "{verified}");
    match install::installed_state(&fake.paths, &fake.state).unwrap() {
        InstalledState::Current(r) => assert_eq!(r.patched, [SETTINGS_LAYOUT, ingame::OWN_SCRIPT]),
        other => panic!("expected Current, got {other:?}"),
    }
    let plan = install::plan(&fake.paths, &HudLayout::default(), &fake.state).unwrap();
    assert_eq!(plan.action, HudAction::Remove);
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    assert!(!fake.addon().exists());
    rebuilt
}

#[test]
fn ingame_rows_rebuild_the_settings_layout() {
    let rebuilt = check_ingame_rows(&Fake::new());
    assert!(rebuilt.contains("<PopupSettingsSettingsRow id=\"DtWideFovRow\">"));
}

/// `DEADTUNE_GAME_SNAPSHOT=<dir>` points at a game file snapshot kept outside the repo
/// (Valve's files): the real settings menu goes through the whole pipeline and the real
/// saved ConVars through the reader. `DEADTUNE_GAME_SNAPSHOT_OUT=<file>` keeps the rebuilt
/// menu's text.
#[test]
fn ingame_rows_rebuild_the_real_settings_layout() {
    let Ok(dir) = std::env::var("DEADTUNE_GAME_SNAPSHOT") else {
        return;
    };
    let dir = Path::new(&dir);
    let raw = fs::read(dir.join("raw").join(SETTINGS_LAYOUT)).unwrap();
    let decoded =
        fs::read_to_string(dir.join("text/panorama/layout/popups/popup_settings.xml")).unwrap();
    assert_eq!(
        inject::layout_text(&raw).unwrap(),
        decoded,
        "the snapshot's own decode"
    );
    let rebuilt = check_ingame_rows(&Fake::build(Some(raw)));
    let camera = rebuilt.find("id=\"citadel_settings_camera\"").unwrap();
    let wide = rebuilt.find("id=\"DtWideFovRow\"").unwrap();
    assert!(
        camera < wide && wide - camera < 1200,
        "inside Camera Settings: {}",
        &rebuilt[camera..wide]
    );
    eprintln!(
        "real popup_settings: {} lines decoded, {} lines rebuilt",
        decoded.lines().count(),
        rebuilt.lines().count()
    );
    if let Ok(out) = std::env::var("DEADTUNE_GAME_SNAPSHOT_OUT") {
        fs::write(out, &rebuilt).unwrap();
    }

    let saved = usercfg::read_convars(&dir.join("raw/cfg")).unwrap();
    assert_eq!(
        saved.get("citadel_camera_hero_fov").map(String::as_str),
        Some("75")
    );
    assert!(saved.len() > 50, "{}", saved.len());
    assert!(
        !saved.contains_key(ingame::STASH_CONVAR),
        "never written by the mod on this machine"
    );
}

#[test]
fn minimap_colors_patch_only_the_minimap_stylesheet() {
    let fake = Fake::new();
    let mut hud = HudLayout::default();
    hud.minimap_colors
        .insert(IconId::EnemyHero, Color([0, 0xD5, 0xFF, 255]));
    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    assert!(matches!(plan.action, HudAction::Write(_)));
    install::execute(&plan, &fake.paths, &fake.state).unwrap();

    let addon = VpkDir::open(&fake.addon()).unwrap();
    assert!(!addon.contains(HUD), "hud.vcss_c untouched");
    let res = Resource::parse(&addon.read(MINIMAP).unwrap()).unwrap();
    assert!(style_text(&res).unwrap().ends_with(
        "#hud_minimap .map_button.player.enemy #BackgroundImage{background-color:#00D5FF;}"
    ));
}

#[test]
fn icon_override_ships_in_the_hud_pak_and_passes_verify() {
    let fake = Fake::new();
    let scope = "panorama/images/hud/crosshair/scope_common_psd.vtex_c";
    fake.update_game_file(scope, vindicta_texture());
    let mut hud = HudLayout::default();
    hud.elements.insert(
        ElementId::Minimap,
        ElementEdit {
            scale_pct: 120,
            ..ElementEdit::default()
        },
    );
    icons::set(
        &mut hud.icons,
        &fake.state,
        scope,
        &test_png(300, 200),
        Fit::Original,
    )
    .unwrap();
    assert!(!hud.is_vanilla());

    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    assert_eq!(plan.icon_problems, []);
    let HudAction::Write(bytes) = &plan.action else {
        panic!("expected a write, got {:?}", plan.action);
    };
    let addon = VpkDir::in_memory(bytes.clone()).unwrap();
    assert!(addon.contains(HUD), "the layout edit ships beside the icon");
    let v = Vtex::parse(&addon.read(scope).unwrap()).unwrap();
    assert_eq!(
        (v.width, v.height, v.format.name(), v.mips.len()),
        (1080, 1080, "BGRA8888", 1)
    );
    assert!(v.flags.contains(Flags::NO_LOD));
    let expect = verify::expect_for_hud(&fake.game(), &addon);
    assert!(matches!(
        expect.checks.get(scope),
        Some(verify::Check::ReplacedImageOf(_))
    ));
    let verified = verify::verify(&addon, &expect);
    assert!(verified.is_ok(), "{verified}");

    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    match install::installed_state(&fake.paths, &fake.state).unwrap() {
        InstalledState::Current(r) => assert!(r.patched.contains(&scope.to_string()), "{r:?}"),
        other => panic!("expected Current, got {other:?}"),
    }
    let reports = verify::verify_installed(&fake.paths, &fake.state);
    assert_eq!(reports.len(), 1);
    assert!(
        reports[0].result.as_ref().is_ok_and(|v| v.is_ok()),
        "{}",
        reports[0]
    );
    assert_eq!(
        install::plan(&fake.paths, &hud, &fake.state)
            .unwrap()
            .action,
        HudAction::Nothing,
        "rebuilding the same icon is byte-identical"
    );
}

#[test]
fn a_broken_icon_is_reported_and_the_rest_still_ships() {
    let fake = Fake::new();
    let mut hud = HudLayout::default();
    let gone = "panorama/images/hud/removed_by_update_psd.vtex_c";
    icons::set(
        &mut hud.icons,
        &fake.state,
        gone,
        &test_png(8, 8),
        Fit::Original,
    )
    .unwrap();

    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    assert_eq!(plan.action, HudAction::Nothing, "nothing else to ship");
    assert_eq!(plan.icon_problems.len(), 1);
    assert_eq!(plan.icon_problems[0].game_path, gone);

    icons::set(&mut hud.icons, &fake.state, ICON, &test_png(8, 8), Fit::Own).unwrap();
    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    assert_eq!(plan.icon_problems.len(), 1);
    assert_eq!(plan.shipped().collect::<Vec<_>>(), [ICON]);
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    let addon = VpkDir::open(&fake.addon()).unwrap();
    assert!(addon.contains(ICON) && !addon.contains(gone));
    let v = Vtex::parse(&addon.read(ICON).unwrap()).unwrap();
    assert_eq!((v.width, v.height), (8, 8));
    match install::installed_state(&fake.paths, &fake.state).unwrap() {
        InstalledState::Current(r) => assert_eq!(r.patched, [ICON]),
        other => panic!("expected Current, got {other:?}"),
    }
}

#[test]
fn a_game_update_rebuilds_icons_from_the_new_file() {
    let fake = Fake::new();
    let mut hud = HudLayout::default();
    icons::set(
        &mut hud.icons,
        &fake.state,
        ICON,
        &test_png(30, 30),
        Fit::Original,
    )
    .unwrap();
    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    let dims = |fake: &Fake| {
        let v = Vtex::parse(&VpkDir::open(&fake.addon()).unwrap().read(ICON).unwrap()).unwrap();
        (v.width, v.height)
    };
    assert_eq!(dims(&fake), (1024, 1024));

    fake.update_game_file(ICON, vindicta_texture());
    write_manifest(fake.paths.appmanifest.as_ref().unwrap(), "20261006");
    assert!(matches!(
        install::installed_state(&fake.paths, &fake.state).unwrap(),
        InstalledState::Stale(_)
    ));
    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    assert!(matches!(plan.action, HudAction::Write(_)));
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    assert_eq!(
        dims(&fake),
        (1080, 1080),
        "sized from the updated game file"
    );
}

#[test]
fn removing_the_hud_removes_icons_and_keeps_the_stored_image() {
    let fake = Fake::new();
    let mut hud = HudLayout::default();
    let set = icons::set(&mut hud.icons, &fake.state, ICON, &test_png(8, 8), Fit::Own).unwrap();
    fake.install(layout::compile(&hud).unwrap());
    assert!(fake.addon().exists());

    let plan = install::plan(&fake.paths, &HudLayout::default(), &fake.state).unwrap();
    assert_eq!(plan.action, HudAction::Remove);
    install::execute(&plan, &fake.paths, &fake.state).unwrap();
    assert!(!fake.addon().exists());
    assert!(set.stored_at(&fake.state).is_file());

    icons::reset(&mut hud.icons, ICON);
    assert!(hud.is_vanilla());
}

#[test]
fn verify_rejects_an_icon_that_is_not_our_encoding() {
    let fake = Fake::new();
    let mut hud = HudLayout::default();
    icons::set(&mut hud.icons, &fake.state, ICON, &test_png(8, 8), Fit::Own).unwrap();
    let plan = install::plan(&fake.paths, &hud, &fake.state).unwrap();
    let HudAction::Write(bytes) = &plan.action else {
        panic!("expected a write");
    };
    let built = VpkDir::in_memory(bytes.clone()).unwrap();
    let mut files: BTreeMap<String, Vec<u8>> = built
        .entries
        .keys()
        .map(|p| (p.clone(), built.read(p).unwrap()))
        .collect();
    files.insert(ICON.to_string(), game_icon());
    let bad = VpkDir::in_memory(vpk::write(&files)).unwrap();
    let verified = verify::verify(&bad, &verify::expect_for_hud(&fake.game(), &bad));
    assert!(
        verified
            .problems
            .iter()
            .any(|p| p.contains("expected one BGRA8888")),
        "{verified}"
    );
    files.insert(ICON.to_string(), {
        let mut ours = built.read(ICON).unwrap();
        ours.pop();
        ours
    });
    let bad = VpkDir::in_memory(vpk::write(&files)).unwrap();
    let verified = verify::verify(&bad, &verify::expect_for_hud(&fake.game(), &bad));
    assert!(!verified.is_ok(), "a truncated icon fails");
}
