use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use dt_core::hud::StylePatch;
use dt_core::hud::install::{self, ADDON_FILE, GAME_PAK, HudAction, HudError, InstalledState};
use dt_core::hud::resource::{Resource, style_text};
use dt_core::hud::vpk::{self, VpkDir, VpkError};
use dt_core::locate::{self, GamePaths};

const HUD: &str = "panorama/styles/hud.vcss_c";
const CSS: &str = "#TopBar{opacity:0.5;}";

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

    fn with_pak(pak: bool) -> Fake {
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
        if pak {
            let mut files = BTreeMap::new();
            files.insert(HUD.to_string(), vanilla_hud());
            files.insert("scripts/unrelated.txt".to_string(), b"unrelated".to_vec());
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

    fn install(&self, patch: StylePatch) {
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

fn vanilla_hud() -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/hud_vanilla.vcss_c"))
        .unwrap()
}

fn patch(css: &str) -> StylePatch {
    StylePatch {
        files: BTreeMap::from([(HUD.to_string(), css.to_string())]),
    }
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
        install::build_addon(&pak, &patch(CSS)).unwrap(),
        install::build_addon(&pak, &patch(CSS)).unwrap()
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
    let plan = install::plan_patch(&fake.paths, StylePatch::default(), &fake.state).unwrap();
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

    for p in [patch(CSS), StylePatch::default()] {
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
    let mut files = p.files;
    files.insert("panorama/styles/nope.vcss_c".to_string(), CSS.to_string());
    let err = install::plan_patch(&fake.paths, StylePatch { files }, &fake.state).unwrap_err();
    assert!(matches!(err, HudError::Vpk(VpkError::Missing(_))), "{err}");
}
