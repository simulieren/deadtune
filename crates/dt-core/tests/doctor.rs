//! `dt_core::doctor` against a tempdir fake install.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use dt_core::doctor::{Check, CheckStatus, hud_conflicts, run};
use dt_core::locate::{GamePaths, from_game_root};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

struct Fake {
    tmp: tempfile::TempDir,
    paths: GamePaths,
}

impl Fake {
    fn new() -> Fake {
        let tmp = tempfile::tempdir().unwrap();
        let steamapps = tmp.path().join("steamapps");
        let root = steamapps.join("common/Deadlock");
        let citadel = root.join("game/citadel");
        fs::create_dir_all(citadel.join("cfg")).unwrap();
        fs::copy(
            repo("research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi"),
            citadel.join("gameinfo.gi"),
        )
        .unwrap();
        fs::copy(
            repo("research/configs/OptimizationLock/test_cfg/video.txt"),
            citadel.join("cfg/video.txt"),
        )
        .unwrap();
        fs::write(
            steamapps.join("appmanifest_1422450.acf"),
            "\"AppState\"\n{\n\t\"buildid\"\t\t\"20261004\"\n}\n",
        )
        .unwrap();
        fs::write(citadel.join("pak01_dir.vpk"), hud_pak()).unwrap();
        let paths = from_game_root(&root).unwrap();
        Fake { tmp, paths }
    }

    fn data(&self) -> PathBuf {
        self.tmp.path().join("data")
    }

    fn run(&self) -> Vec<Check> {
        run(Some(&self.paths), &self.data())
    }
}

fn hud_pak() -> Vec<u8> {
    let hud = fs::read(repo("crates/dt-core/tests/fixtures/hud/hud_vanilla.vcss_c")).unwrap();
    dt_core::hud::vpk::write(&BTreeMap::from([(
        "panorama/styles/hud.vcss_c".to_string(),
        hud,
    )]))
}

fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
    checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check {name:?} in {checks:#?}"))
}

fn tree(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(tree(&path));
        }
        out.push(path);
    }
    out.sort();
    out
}

#[test]
fn healthy_install_has_no_failures_and_leaves_the_game_untouched() {
    let fake = Fake::new();
    let game_before = tree(&fake.paths.game_root);
    let gameinfo_before = fs::read(&fake.paths.gameinfo).unwrap();
    let checks = fake.run();
    let failed: Vec<_> = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Fail)
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
    for name in [
        "Find Deadlock",
        "Read gameinfo.gi",
        "Line endings",
        "Brace balance",
        "Lossless edit",
        "Read convars",
        "Read video.txt",
        "Game build id",
        "Write cfg folder",
        "Game archive (HUD)",
        "HUD addon",
        "Performance addons",
        "DeadTune data folder",
        "Backups",
        "Live console (netcon)",
        "Game running",
    ] {
        find(&checks, name);
    }
    assert_eq!(find(&checks, "Game build id").detail, "20261004");
    assert_eq!(find(&checks, "Lossless edit").status, CheckStatus::Pass);
    assert_eq!(
        tree(&fake.paths.game_root),
        game_before,
        "no files left behind"
    );
    assert_eq!(fs::read(&fake.paths.gameinfo).unwrap(), gameinfo_before);
    for c in &checks {
        if c.status != CheckStatus::Pass {
            assert!(c.fix.is_some(), "{} is not Pass and needs a fix", c.name);
        }
    }
}

#[test]
fn missing_game_fails_with_a_plain_fix() {
    let tmp = tempfile::tempdir().unwrap();
    let checks = run(None, tmp.path());
    let locate = find(&checks, "Find Deadlock");
    assert_eq!(locate.status, CheckStatus::Fail);
    assert!(locate.fix.as_deref().unwrap().contains("Steam"));
    assert_eq!(
        find(&checks, "DeadTune data folder").status,
        CheckStatus::Pass,
        "checks that do not need the game still run"
    );
}

#[test]
fn broken_gameinfo_fails_brace_check() {
    let fake = Fake::new();
    let text = fs::read_to_string(&fake.paths.gameinfo).unwrap() + "{\n";
    fs::write(&fake.paths.gameinfo, text).unwrap();
    let checks = fake.run();
    let braces = find(&checks, "Brace balance");
    assert_eq!(braces.status, CheckStatus::Fail);
    assert!(braces.fix.as_deref().unwrap().contains("Verify"));
}

#[test]
fn missing_video_and_pak_warn_but_do_not_fail() {
    let fake = Fake::new();
    fs::remove_file(&fake.paths.video).unwrap();
    fs::remove_file(fake.paths.citadel_dir.join("pak01_dir.vpk")).unwrap();
    let checks = fake.run();
    assert_eq!(find(&checks, "Read video.txt").status, CheckStatus::Warn);
    assert_eq!(
        find(&checks, "Game archive (HUD)").status,
        CheckStatus::Warn
    );
    assert!(
        checks.iter().all(|c| c.status != CheckStatus::Fail),
        "{checks:#?}"
    );
}

#[test]
fn unwritable_data_dir_fails() {
    let fake = Fake::new();
    let not_a_dir = fake.tmp.path().join("file");
    fs::write(&not_a_dir, "x").unwrap();
    let checks = run(Some(&fake.paths), &not_a_dir);
    assert_eq!(
        find(&checks, "DeadTune data folder").status,
        CheckStatus::Fail
    );
}

#[test]
fn another_hud_mod_is_a_warning() {
    let fake = Fake::new();
    let addons = fake.paths.citadel_dir.join("addons");
    fs::create_dir_all(&addons).unwrap();
    fs::write(addons.join("pak03_dir.vpk"), hud_pak()).unwrap();
    fs::copy(
        repo("crates/dt-core/tests/fixtures/hud/blur_pak97_dir.vpk"),
        addons.join("pak97_dir.vpk"),
    )
    .unwrap();
    assert_eq!(
        hud_conflicts(&fake.paths),
        vec![addons.join("pak03_dir.vpk")]
    );
    let checks = fake.run();
    let conflict = find(&checks, "HUD conflicts");
    assert_eq!(conflict.status, CheckStatus::Warn);
    assert!(conflict.detail.contains("pak03_dir.vpk"));
}
