//! `dt_core::apply` against the real presets and a tempdir fake install.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use dt_core::apply::*;
use dt_core::backup::{BackupStore, FileKind};
use dt_core::bridge::{Bridge, BridgeError, ConsoleCmd};
use dt_core::catalog::{ApplyClass, Catalog};
use dt_core::gi::{effective_values, read_convars};
use dt_core::hud::install::{ADDON_FILE, GAME_PAK, HudAction, addons_dir};
use dt_core::hud::searchpaths::has_addons;
use dt_core::hud::{ElementEdit, ElementId, HudLayout};
use dt_core::locate::{GamePaths, from_game_root};
use dt_core::preset::{self, PresetId};
use dt_core::profile::{BaseRef, ConVarEdits, Profile};

const VANILLA: &str =
    include_str!("../../../research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi");
const KAIZ: &str = include_str!(
    "../../../research/configs/OptimizationLock/kaizuchanerus minimum spec/gameinfo.gi"
);
const LIVE_VIDEO: &str =
    include_str!("../../../research/configs/OptimizationLock/test_cfg/video.txt");

const CHEAT_SET: &str = "csm_max_visible_dist";
const LIVE_COMMENTED: &str = "citadel_damage_offscreen_indicator_disabled";
const DENIED: &str = "citadel_player_outline_enemies";

fn catalog() -> &'static Catalog {
    Catalog::embedded()
}

fn kaiz_profile() -> Profile {
    Profile {
        name: "laptop".into(),
        base: BaseRef::Preset(PresetId::KaizMinspec),
        base_rev: None,
        convars: ConVarEdits {
            set: BTreeMap::from([
                (CHEAT_SET.to_string(), "2000".to_string()),
                (DENIED.to_string(), "1".to_string()),
            ]),
            comment: vec![LIVE_COMMENTED.to_string()],
        },
        video: BTreeMap::new(),
        hud: HudLayout::default(),
    }
}

struct FakeInstall {
    _dir: tempfile::TempDir,
    paths: GamePaths,
    store: BackupStore,
}

fn fake_install(gameinfo: &str, video: Option<&str>) -> FakeInstall {
    let dir = tempfile::tempdir().unwrap();
    let game_root = dir.path().join("steamapps/common/Deadlock");
    let citadel = game_root.join("game/citadel");
    fs::create_dir_all(citadel.join("cfg")).unwrap();
    fs::write(citadel.join("gameinfo.gi"), gameinfo).unwrap();
    if let Some(video) = video {
        fs::write(citadel.join("cfg/video.txt"), video).unwrap();
    }
    let paths = from_game_root(&game_root).unwrap();
    let store = BackupStore::open(dir.path().join("data")).unwrap();
    FakeInstall {
        _dir: dir,
        paths,
        store,
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn read_opt(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Everything outside the ConVars block, which apply must never touch.
fn outside_convars(text: &str) -> String {
    let start = text.find("ConVars").unwrap();
    let close = text[start..].find("\n    }").unwrap() + start;
    format!("{}{}", &text[..start], &text[close..])
}

#[derive(Default)]
struct RecordingBridge {
    pushed: Vec<ConsoleCmd>,
    fail: bool,
}

impl Bridge for RecordingBridge {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn push(&mut self, cmds: &[ConsoleCmd]) -> Result<(), BridgeError> {
        if self.fail {
            return Err(BridgeError::Io(std::io::Error::other("netcon refused")));
        }
        self.pushed.extend_from_slice(cmds);
        Ok(())
    }
}

fn plan_for(install: &FakeInstall, profile: &Profile, ctx: ApplyContext) -> ApplyPlan {
    let live = read(&install.paths.gameinfo);
    let live_video = read_opt(&install.paths.video);
    let base = resolve_base(profile).unwrap();
    let tgt = target(
        &live,
        live_video.as_deref(),
        &base,
        profile,
        catalog(),
        None,
    )
    .unwrap();
    plan(
        &install.paths,
        &live,
        live_video.as_deref(),
        &tgt,
        catalog(),
        ctx,
    )
    .unwrap()
}

#[test]
fn resolve_base_uses_pinned_preset_text_or_the_base_file() {
    let pinned = resolve_base(&kaiz_profile()).unwrap();
    assert_eq!(
        pinned.gameinfo,
        preset::info(PresetId::KaizMinspec).pinned_gameinfo
    );
    assert_eq!(pinned.video, None, "kaiz ships no video.txt");

    let potato = Profile {
        base: BaseRef::Preset(PresetId::OptilockPotato),
        ..kaiz_profile()
    };
    assert!(resolve_base(&potato).unwrap().video.is_some());

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("mine.gi");
    fs::write(&file, KAIZ).unwrap();
    let from_file = Profile {
        base: BaseRef::File(file.clone()),
        ..kaiz_profile()
    };
    assert_eq!(resolve_base(&from_file).unwrap().gameinfo, KAIZ);

    let missing = Profile {
        base: BaseRef::File(dir.path().join("nope.gi")),
        ..kaiz_profile()
    };
    assert!(matches!(
        resolve_base(&missing),
        Err(ApplyError::Base(path, _)) if path.ends_with("nope.gi")
    ));
}

#[test]
fn target_takes_base_block_plus_edits_and_keeps_live_outside_convars() {
    let profile = kaiz_profile();
    let base = resolve_base(&profile).unwrap();
    let tgt = target(VANILLA, None, &base, &profile, catalog(), None).unwrap();

    let effective = effective_values(&tgt.gameinfo).unwrap();
    let base_effective = effective_values(KAIZ).unwrap();
    assert_eq!(effective[CHEAT_SET], "2000");
    assert!(
        !effective.contains_key(LIVE_COMMENTED),
        "comment edit applied"
    );
    assert!(base_effective.contains_key(LIVE_COMMENTED));
    assert!(
        !effective.contains_key(DENIED),
        "denylisted set is never written"
    );
    assert_eq!(tgt.denied, vec![DENIED.to_string()]);
    for (name, value) in &base_effective {
        if name != CHEAT_SET && name != LIVE_COMMENTED {
            assert_eq!(
                effective.get(name),
                Some(value),
                "{name} comes from the base"
            );
        }
    }
    assert_eq!(outside_convars(&tgt.gameinfo), outside_convars(VANILLA));
    assert!(!tgt.gameinfo.contains('\r'), "live file's LF endings win");
    assert_eq!(tgt.video, None, "no live video, no video target");
}

#[test]
fn target_video_swaps_base_settings_then_applies_profile_video() {
    let profile = Profile {
        base: BaseRef::Preset(PresetId::OptilockPotato),
        video: BTreeMap::from([("setting.fps_max".to_string(), "60".to_string())]),
        ..kaiz_profile()
    };
    let base = resolve_base(&profile).unwrap();
    let tgt = target(VANILLA, Some(LIVE_VIDEO), &base, &profile, catalog(), None).unwrap();
    let video = tgt.video.unwrap();
    let settings: BTreeMap<_, _> = dt_core::video::read_settings(&video)
        .unwrap()
        .into_iter()
        .collect();
    let base_settings: BTreeMap<_, _> =
        dt_core::video::read_settings(base.video.as_deref().unwrap())
            .unwrap()
            .into_iter()
            .collect();
    assert_eq!(settings["setting.fps_max"], "60");
    assert_eq!(
        settings["setting.defaultres"], base_settings["setting.defaultres"],
        "base video settings replace live ones"
    );
    assert!(video.starts_with("\"video.cfg\""), "live header kept");

    let kaiz_video_edit = Profile {
        video: BTreeMap::from([("setting.fps_max".to_string(), "60".to_string())]),
        ..kaiz_profile()
    };
    let base = resolve_base(&kaiz_video_edit).unwrap();
    let tgt = target(
        VANILLA,
        Some(LIVE_VIDEO),
        &base,
        &kaiz_video_edit,
        catalog(),
        None,
    )
    .unwrap();
    let video = tgt.video.unwrap();
    assert!(video.contains("\"setting.fps_max\"\t\t\"60\""));
    assert!(
        video.contains("\"setting.defaultres\"\t\t\"1920\""),
        "base without video keeps live settings"
    );
}

#[test]
fn plan_buckets_follow_catalog_apply_classes() {
    let install = fake_install(VANILLA, None);
    let profile = kaiz_profile();
    let live_eff = effective_values(VANILLA).unwrap();
    let base = resolve_base(&profile).unwrap();
    let target_eff = effective_values(
        &target(VANILLA, None, &base, &profile, catalog(), None)
            .unwrap()
            .gameinfo,
    )
    .unwrap();

    for in_sandbox in [false, true] {
        let ctx = ApplyContext {
            in_sandbox,
            game_running: true,
        };
        let plan = plan_for(&install, &profile, ctx);
        let live: BTreeMap<_, _> = plan
            .live
            .iter()
            .map(|c| (c.name.as_str(), c.value.as_str()))
            .collect();
        let changed = target_eff
            .iter()
            .filter(|(name, value)| live_eff.get(*name) != Some(value))
            .map(|(name, value)| (name, Some(value.clone())));
        let removed = live_eff
            .keys()
            .filter(|name| !target_eff.contains_key(*name))
            .map(|name| (name, catalog().get(name).and_then(|e| e.default.clone())));
        let mut seen = 0;
        let mut removed_count = 0;
        for (name, value) in changed.chain(removed) {
            seen += 1;
            removed_count += usize::from(!target_eff.contains_key(name));
            let in_live = value
                .as_deref()
                .is_some_and(|v| live.get(name.as_str()) == Some(&v));
            let queued = plan.queued_cheat.contains(name);
            let restart = plan.restart.contains(name);
            match (value.is_some(), catalog().apply_class(name), in_sandbox) {
                (false, _, _) => {
                    assert!(
                        restart && !in_live && !queued,
                        "{name}: removed, no default"
                    )
                }
                (true, ApplyClass::Live, _) | (true, ApplyClass::LiveCheat, true) => {
                    assert!(
                        in_live && !queued && !restart,
                        "{name} should be pushed live"
                    )
                }
                (true, ApplyClass::LiveCheat, false) => {
                    assert!(queued && !in_live && !restart, "{name} should be queued")
                }
                (true, ApplyClass::Restart, _) => {
                    assert!(
                        restart && !in_live && !queued,
                        "{name} should need a restart"
                    )
                }
            }
        }
        assert!(
            seen > 100,
            "kaiz changes many convars vs vanilla, saw {seen}"
        );
        assert!(removed_count > 0, "kaiz comments out some stock convars");
        assert_eq!(
            live.len() + plan.queued_cheat.len() + plan.restart.len(),
            seen,
            "every change lands in exactly one bucket"
        );
        assert_eq!(plan.denied, vec![DENIED.to_string()]);
        let write = plan.gameinfo.as_ref().unwrap();
        assert_eq!(write.path, install.paths.gameinfo);
        assert_eq!(write.before, VANILLA);
        assert!(plan.video.is_none() && plan.video_changes.is_empty());
        if in_sandbox {
            assert_eq!(live.get(CHEAT_SET), Some(&"2000"));
        } else {
            assert!(plan.queued_cheat.iter().any(|n| n == CHEAT_SET));
        }
    }
}

#[test]
fn commenting_out_a_live_convar_pushes_its_catalog_default() {
    let profile = Profile {
        convars: ConVarEdits::default(),
        ..kaiz_profile()
    };
    let base = resolve_base(&profile).unwrap();
    let applied = target(VANILLA, None, &base, &profile, catalog(), None).unwrap();
    let install = fake_install(&applied.gameinfo, None);

    let unknown_restart = read_convars(&applied.gameinfo)
        .unwrap()
        .into_iter()
        .find(|e| !e.commented && catalog().get(&e.name).is_none())
        .map(|e| e.name);
    let mut comment = vec![LIVE_COMMENTED.to_string()];
    comment.extend(unknown_restart.clone());
    let commenting = Profile {
        convars: ConVarEdits {
            set: BTreeMap::new(),
            comment,
        },
        ..kaiz_profile()
    };
    let plan = plan_for(&install, &commenting, ApplyContext::default());
    let default = catalog()
        .get(LIVE_COMMENTED)
        .unwrap()
        .default
        .clone()
        .unwrap();
    assert_eq!(catalog().apply_class(LIVE_COMMENTED), ApplyClass::Live);
    assert_eq!(
        plan.live,
        vec![ConsoleCmd {
            name: LIVE_COMMENTED.into(),
            value: default
        }]
    );
    if let Some(name) = unknown_restart {
        assert_eq!(plan.restart, vec![name], "no default known: restart");
    }
}

#[test]
fn video_plan_lists_changed_keys() {
    let install = fake_install(VANILLA, Some(LIVE_VIDEO));
    let profile = Profile {
        convars: ConVarEdits::default(),
        video: BTreeMap::from([
            ("setting.fps_max".to_string(), "60".to_string()),
            ("setting.cpu_level".to_string(), "0".to_string()),
        ]),
        ..kaiz_profile()
    };
    let plan = plan_for(&install, &profile, ApplyContext::default());
    assert_eq!(
        plan.video_changes,
        BTreeMap::from([("setting.fps_max".to_string(), "60".to_string())]),
        "cpu_level is already 0 in the live file"
    );
    let write = plan.video.unwrap();
    assert_eq!(write.path, install.paths.video);
    assert_eq!(write.before, LIVE_VIDEO);
}

#[test]
fn execute_writes_backs_up_snapshots_and_pushes_then_reapply_is_a_no_op() {
    let install = fake_install(VANILLA, Some(LIVE_VIDEO));
    let profile = Profile {
        video: BTreeMap::from([("setting.fps_max".to_string(), "60".to_string())]),
        ..kaiz_profile()
    };
    let ctx = ApplyContext {
        in_sandbox: true,
        game_running: true,
    };
    let plan = plan_for(&install, &profile, ctx);
    let mut bridge = RecordingBridge::default();
    let report = execute(&install.paths, &plan, &install.store, Some(&mut bridge)).unwrap();

    assert_eq!(
        report,
        ApplyReport {
            wrote_gameinfo: true,
            wrote_video: true,
            pushed_live: plan.live.len(),
            needs_restart: true,
            bridge_error: None,
            hud_changed: false,
        }
    );
    assert_eq!(bridge.pushed, plan.live);
    assert_eq!(
        read(&install.paths.gameinfo),
        plan.gameinfo.as_ref().unwrap().after
    );
    assert_eq!(
        read(&install.paths.video),
        plan.video.as_ref().unwrap().after
    );

    let original = install.store.original(FileKind::GameInfo).unwrap();
    assert_eq!(read(&original.path), VANILLA);
    assert_eq!(
        read(&install.store.original(FileKind::Video).unwrap().path),
        LIVE_VIDEO
    );
    let backups = install.store.list(FileKind::GameInfo).unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(read(&backups[0].path), VANILLA);

    let again = plan_for(&install, &profile, ctx);
    assert!(
        again.is_empty(),
        "second apply has nothing to do: {again:?}"
    );
    assert!(again.restart.is_empty() && again.queued_cheat.is_empty());
    let report = execute(&install.paths, &again, &install.store, Some(&mut bridge)).unwrap();
    assert!(!report.wrote_gameinfo && !report.wrote_video && report.pushed_live == 0);
    assert_eq!(install.store.list(FileKind::GameInfo).unwrap().len(), 1);
    assert_eq!(
        read(&install.store.original(FileKind::GameInfo).unwrap().path),
        VANILLA,
        "original is never overwritten"
    );
}

#[test]
fn bridge_failure_after_writing_reports_what_was_written() {
    let install = fake_install(VANILLA, None);
    let ctx = ApplyContext {
        in_sandbox: true,
        game_running: true,
    };
    let plan = plan_for(&install, &kaiz_profile(), ctx);
    assert!(!plan.live.is_empty());
    let mut bridge = RecordingBridge {
        fail: true,
        ..Default::default()
    };
    let report = execute(&install.paths, &plan, &install.store, Some(&mut bridge)).unwrap();
    assert!(report.wrote_gameinfo);
    assert_eq!(report.pushed_live, 0);
    assert!(
        report.needs_restart,
        "unpushed live changes wait for a restart"
    );
    assert!(report.bridge_error.unwrap().contains("netcon refused"));
    assert_eq!(read(&install.paths.gameinfo), plan.gameinfo.unwrap().after);
}

#[test]
fn execute_without_bridge_marks_live_changes_as_needing_restart() {
    let install = fake_install(VANILLA, None);
    let ctx = ApplyContext {
        in_sandbox: true,
        game_running: false,
    };
    let plan = plan_for(&install, &kaiz_profile(), ctx);
    let report = execute(&install.paths, &plan, &install.store, None).unwrap();
    assert_eq!(report.pushed_live, 0);
    assert!(report.needs_restart);
    assert_eq!(report.bridge_error, None);
}

#[test]
fn execute_refuses_when_the_live_file_changed_since_planning() {
    let install = fake_install(VANILLA, None);
    let plan = plan_for(&install, &kaiz_profile(), ApplyContext::default());
    let updated = VANILLA.replace("sv_minrate                   \"98304\"", "sv_minrate \"1\"");
    fs::write(&install.paths.gameinfo, &updated).unwrap();
    assert!(matches!(
        execute(&install.paths, &plan, &install.store, None),
        Err(ApplyError::Stale(path)) if path == install.paths.gameinfo
    ));
    assert_eq!(read(&install.paths.gameinfo), updated, "nothing written");
}

#[test]
fn execute_refuses_unbalanced_gameinfo() {
    let install = fake_install(VANILLA, None);
    let mut plan = plan_for(&install, &kaiz_profile(), ApplyContext::default());
    plan.gameinfo.as_mut().unwrap().after.push_str("{\n");
    assert!(matches!(
        execute(&install.paths, &plan, &install.store, None),
        Err(ApplyError::Gi(_))
    ));
    assert_eq!(read(&install.paths.gameinfo), VANILLA);
}

#[test]
fn ranked_safe_restores_stock_block_and_keeps_modified_search_paths() {
    let modded = VANILLA.replacen(
        "Game_UILanguage",
        "Game citadel/addons // mod manager\n            Game_UILanguage",
        1,
    );
    let profile = kaiz_profile();
    let base = resolve_base(&profile).unwrap();
    let tuned = target(&modded, None, &base, &profile, catalog(), None).unwrap();
    let install = fake_install(&tuned.gameinfo, None);

    let safe = ranked_safe_target(&tuned.gameinfo, &install.store, None).unwrap();
    assert_eq!(safe.video, None);
    assert!(safe.gameinfo.contains("citadel/addons // mod manager"));
    assert_eq!(
        safe.gameinfo, modded,
        "no original yet: vanilla preset block"
    );

    let plan = plan(
        &install.paths,
        &tuned.gameinfo,
        None,
        &safe,
        catalog(),
        ApplyContext::default(),
    )
    .unwrap();
    assert!(plan.gameinfo.is_some() && plan.video.is_none());

    let original_dir = tempfile::tempdir().unwrap();
    let original = original_dir.path().join("gameinfo.gi");
    let custom_stock = VANILLA.replace("\"98304\"", "\"12345\"");
    fs::write(&original, &custom_stock).unwrap();
    install
        .store
        .snapshot_original(FileKind::GameInfo, &original)
        .unwrap();
    let safe = ranked_safe_target(&tuned.gameinfo, &install.store, None).unwrap();
    assert_eq!(
        effective_values(&safe.gameinfo).unwrap(),
        effective_values(&custom_stock).unwrap(),
        "original snapshot wins over the vanilla preset"
    );
    assert!(safe.gameinfo.contains("citadel/addons // mod manager"));
}

#[test]
fn unified_diff_shows_changed_lines() {
    let write = FileWrite {
        path: "gameinfo.gi".into(),
        before: "a\nb\nc\n".into(),
        after: "a\nB\nc\n".into(),
    };
    let diff = write.unified_diff();
    assert!(diff.contains("-b\n") && diff.contains("+B\n"), "{diff}");
    assert!(diff.contains("gameinfo.gi"));
}

fn with_game_pak(install: &FakeInstall) {
    let hud = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/hud_vanilla.vcss_c"),
    )
    .unwrap();
    let files = BTreeMap::from([("panorama/styles/hud.vcss_c".to_string(), hud)]);
    fs::write(
        install.paths.citadel_dir.join(GAME_PAK),
        dt_core::hud::vpk::write(&files),
    )
    .unwrap();
}

fn hud_profile() -> Profile {
    let mut profile = kaiz_profile();
    profile.hud.elements.insert(
        ElementId::Minimap,
        ElementEdit {
            scale_pct: 120,
            ..ElementEdit::default()
        },
    );
    profile
}

fn plan_with_hud(install: &FakeInstall, profile: &Profile) -> ApplyPlan {
    let live = read(&install.paths.gameinfo);
    let hud = hud_plan(&install.paths, &profile.hud, &install.store).unwrap();
    let base = resolve_base(profile).unwrap();
    let tgt = target(&live, None, &base, profile, catalog(), hud).unwrap();
    plan(
        &install.paths,
        &live,
        None,
        &tgt,
        catalog(),
        ApplyContext::default(),
    )
    .unwrap()
}

#[test]
fn vanilla_hud_without_our_addon_plans_nothing_even_next_to_a_foreign_pak77() {
    let install = fake_install(VANILLA, None);
    assert_eq!(
        hud_plan(&install.paths, &HudLayout::default(), &install.store).unwrap(),
        None
    );

    let addons = addons_dir(&install.paths);
    fs::create_dir_all(&addons).unwrap();
    fs::write(addons.join(ADDON_FILE), b"someone else's mod").unwrap();
    assert_eq!(
        hud_plan(&install.paths, &HudLayout::default(), &install.store).unwrap(),
        None,
        "a foreign file at our path never blocks a convar-only apply"
    );
}

#[test]
fn hud_layout_installs_addon_and_mounts_addons_search_path_then_converges() {
    let install = fake_install(VANILLA, None);
    with_game_pak(&install);
    let profile = hud_profile();
    let plan = plan_with_hud(&install, &profile);
    let hud = plan.hud.as_ref().unwrap();
    assert!(matches!(hud.action, HudAction::Write(_)));
    assert!(hud.needs_search_path);
    let after = &plan.gameinfo.as_ref().unwrap().after;
    assert!(has_addons(after).unwrap(), "target mounts citadel/addons");

    let report = execute(&install.paths, &plan, &install.store, None).unwrap();
    assert!(report.hud_changed && report.wrote_gameinfo);
    assert!(addons_dir(&install.paths).join(ADDON_FILE).is_file());
    assert!(has_addons(&read(&install.paths.gameinfo)).unwrap());

    let again = plan_with_hud(&install, &profile);
    assert!(
        again.is_empty(),
        "second apply has nothing to do: {again:?}"
    );
    assert_eq!(again.hud.unwrap().action, HudAction::Nothing);
}

#[test]
fn ranked_safe_removes_our_addon_and_keeps_the_search_path_line() {
    let install = fake_install(VANILLA, None);
    with_game_pak(&install);
    let applied = plan_with_hud(&install, &hud_profile());
    execute(&install.paths, &applied, &install.store, None).unwrap();

    let live = read(&install.paths.gameinfo);
    let hud = hud_plan(&install.paths, &HudLayout::default(), &install.store).unwrap();
    assert_eq!(hud.as_ref().unwrap().action, HudAction::Remove);
    let safe = ranked_safe_target(&live, &install.store, hud).unwrap();
    let plan = plan(
        &install.paths,
        &live,
        None,
        &safe,
        catalog(),
        ApplyContext::default(),
    )
    .unwrap();
    let report = execute(&install.paths, &plan, &install.store, None).unwrap();

    assert!(report.hud_changed);
    assert!(!addons_dir(&install.paths).join(ADDON_FILE).exists());
    let gameinfo = read(&install.paths.gameinfo);
    assert_eq!(
        effective_values(&gameinfo).unwrap(),
        effective_values(VANILLA).unwrap()
    );
    assert!(
        has_addons(&gameinfo).unwrap() && addons_dir(&install.paths).is_dir(),
        "the mount line stays and points at an existing, now empty, addons dir"
    );
}
