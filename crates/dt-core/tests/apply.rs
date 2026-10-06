//! `dt_core::apply` against the real presets and a tempdir fake install.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use dt_core::addons::AddonsConfig;
use dt_core::apply::*;
use dt_core::backup::{BackupStore, FileKind};
use dt_core::bridge::{Bridge, BridgeError, ConsoleCmd};
use dt_core::catalog::{ApplyClass, Catalog};
use dt_core::gi::{effective_values, read_convars};
use dt_core::hud::install::{ADDON_FILE, GAME_PAK, HudAction, addons_dir};
use dt_core::hud::searchpaths::has_addons;
use dt_core::hud::{ElementEdit, ElementId, HudLayout};
use dt_core::locate::{GamePaths, from_game_root};
use dt_core::practice::{self, PracticeMode, Record};
use dt_core::preset::{self, PresetId, remote};
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

fn no_cache() -> &'static Path {
    Path::new("no-such-presets-dir")
}

const SIDELOCK_STANDIN_ZIP: &[u8] = include_bytes!("fixtures/sidelock_standin/cfg.zip");

/// A presets cache holding the synthetic SideLock stand-in, accepted as an upstream update
/// (its sha256 is not the pinned one).
fn sidelock_cache() -> tempfile::TempDir {
    let presets = tempfile::tempdir().unwrap();
    let r = preset::remote(PresetId::SideLock);
    let dir = remote::dir(r, presets.path());
    let status = remote::stage(r, &dir, SIDELOCK_STANDIN_ZIP).unwrap();
    assert!(!status.ready() && status.pending.is_some());
    remote::accept(&dir).unwrap();
    presets
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
        addons: AddonsConfig::default(),
        practice: PracticeMode::default(),
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

    /// Records the `name "value"` lines and skips the log redirect and ack trailer.
    fn send(&mut self, lines: &[String]) -> Result<(), BridgeError> {
        if self.fail {
            return Err(BridgeError::Io(std::io::Error::other("netcon refused")));
        }
        self.pushed.extend(lines.iter().filter_map(|line| {
            let (name, value) = line.split_once(" \"")?;
            Some(ConsoleCmd {
                name: name.to_string(),
                value: value.trim_end_matches('"').to_string(),
            })
        }));
        Ok(())
    }
}

fn plan_for(install: &FakeInstall, profile: &Profile, ctx: ApplyContext) -> ApplyPlan {
    let live = read(&install.paths.gameinfo);
    let live_video = read_opt(&install.paths.video);
    let base = resolve_base(profile, no_cache()).unwrap();
    let addons = addons_plan(&install.paths, &profile.addons, &install.store).unwrap();
    let tgt = target(
        &live,
        live_video.as_deref(),
        &base,
        profile,
        catalog(),
        Extras {
            hud: None,
            addons,
            practice: Record::load(&install.store.root).unwrap(),
        },
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
    let pinned = resolve_base(&kaiz_profile(), no_cache()).unwrap();
    assert_eq!(pinned.gameinfo, KAIZ);
    assert_eq!(pinned.video, None, "kaiz ships no video.txt");

    let potato = Profile {
        base: BaseRef::Preset(PresetId::OptilockPotato),
        ..kaiz_profile()
    };
    assert!(resolve_base(&potato, no_cache()).unwrap().video.is_some());

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("mine.gi");
    fs::write(&file, KAIZ).unwrap();
    let from_file = Profile {
        base: BaseRef::File(file.clone()),
        ..kaiz_profile()
    };
    assert_eq!(resolve_base(&from_file, no_cache()).unwrap().gameinfo, KAIZ);

    let missing = Profile {
        base: BaseRef::File(dir.path().join("nope.gi")),
        ..kaiz_profile()
    };
    assert!(matches!(
        resolve_base(&missing, no_cache()),
        Err(ApplyError::Base(path, _)) if path.ends_with("nope.gi")
    ));
}

fn sidelock_profile() -> Profile {
    Profile {
        base: BaseRef::Preset(PresetId::SideLock),
        convars: ConVarEdits::default(),
        ..kaiz_profile()
    }
}

#[test]
fn resolve_base_needs_a_downloaded_or_imported_remote_preset() {
    let empty = tempfile::tempdir().unwrap();
    let err = resolve_base(&sidelock_profile(), empty.path()).unwrap_err();
    assert!(matches!(err, ApplyError::Remote(_)), "{err:?}");
    assert_eq!(
        err.to_string(),
        "SideLock isn't downloaded yet. Download it or import cfg.zip or gameinfo.gi."
    );

    let presets = sidelock_cache();
    let base = resolve_base(&sidelock_profile(), presets.path()).unwrap();
    assert!(base.gameinfo.contains("citadel_damage_indicator_radius"));
    assert_eq!(base.video, None, "remote presets bring no video.txt");
}

#[test]
fn remote_preset_uses_only_its_convars_block_with_the_denylist_and_ignored_marks() {
    let presets = sidelock_cache();
    let profile = sidelock_profile();
    let base = resolve_base(&profile, presets.path()).unwrap();
    let tgt = target(VANILLA, None, &base, &profile, catalog(), Extras::default()).unwrap();

    assert_eq!(
        outside_convars(&tgt.gameinfo),
        outside_convars(VANILLA),
        "its FileSystem, RenderSystem and SceneSystem edits are not applied"
    );
    assert!(!tgt.gameinfo.contains("VulkanUseSecondaryCommandBuffers"));
    assert!(!tgt.gameinfo.contains("CSMCascadeResolution 0"));

    assert_eq!(
        tgt.denied,
        [
            "citadel_player_outline_fade_range_min",
            "r_citadel_selection_outline2_alpha",
            "r_citadel_selection_outline2_fade_pow",
        ]
    );
    let effective = effective_values(&tgt.gameinfo).unwrap();
    let stock = effective_values(VANILLA).unwrap();
    for name in &tgt.denied {
        assert_eq!(effective.get(name), stock.get(name), "{name} back to stock");
    }
    assert_eq!(effective["citadel_damage_indicator_radius"], "1");
    assert_eq!(effective["r_ssao"], "false");

    let install = fake_install(VANILLA, None);
    let plan = plan(
        &install.paths,
        VANILLA,
        None,
        &tgt,
        catalog(),
        ApplyContext::default(),
    )
    .unwrap();
    assert!(plan.ignored.contains(&"r_shadows".to_string()), "{plan:?}");
}

#[test]
fn target_takes_base_block_plus_edits_and_keeps_live_outside_convars() {
    let profile = kaiz_profile();
    let base = resolve_base(&profile, no_cache()).unwrap();
    let tgt = target(VANILLA, None, &base, &profile, catalog(), Extras::default()).unwrap();

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
    assert_eq!(
        tgt.denied,
        [
            DENIED,
            "citadel_trooper_outline_enabled",
            "citadel_use_pvs_for_players",
            "cl_glow_brightness",
            "minimap_trooper_update_rate_hz",
            "r_citadel_npr_force_solid_outline",
        ],
        "the profile edit plus the denylisted values kaiz sets, quoted names included"
    );
    let stock = effective_values(VANILLA).unwrap();
    for name in &tgt.denied {
        assert_eq!(
            effective.get(name),
            stock.get(name),
            "{name} is back to stock"
        );
    }
    for (name, value) in &base_effective {
        if name != CHEAT_SET && name != LIVE_COMMENTED && !tgt.denied.contains(name) {
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
    let base = resolve_base(&profile, no_cache()).unwrap();
    let tgt = target(
        VANILLA,
        Some(LIVE_VIDEO),
        &base,
        &profile,
        catalog(),
        Extras::default(),
    )
    .unwrap();
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
    let base = resolve_base(&kaiz_video_edit, no_cache()).unwrap();
    let tgt = target(
        VANILLA,
        Some(LIVE_VIDEO),
        &base,
        &kaiz_video_edit,
        catalog(),
        Extras::default(),
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
    let live_eff = effective_values(VANILLA).unwrap();
    let mut profile = kaiz_profile();
    let stock = live_eff.keys().find(|n| !catalog().is_denied(n)).unwrap();
    profile.convars.comment.push(stock.clone());
    let base = resolve_base(&profile, no_cache()).unwrap();
    let target_eff = effective_values(
        &target(VANILLA, None, &base, &profile, catalog(), Extras::default())
            .unwrap()
            .gameinfo,
    )
    .unwrap();

    for in_sandbox in [false, true] {
        let ctx = ApplyContext {
            in_sandbox,
            game_running: true,
            waiting_paks: WaitingPaks::None,
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
            let ignored = plan.ignored.contains(name);
            let class = catalog().apply_class(name);
            let pushed = value.is_some()
                && (class == ApplyClass::Live || (class == ApplyClass::LiveCheat && in_sandbox));
            if !pushed && catalog().is_gameinfo_ignored(name) {
                assert!(
                    ignored && !in_live && !queued && !restart,
                    "{name}: the game ignores it in gameinfo.gi"
                );
                continue;
            }
            assert!(!ignored, "{name} is not flagged gameinfo_cannot_override");
            match (value.is_some(), class, in_sandbox) {
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
        assert!(removed_count > 0, "the profile comments out a stock convar");
        assert_eq!(
            live.len() + plan.queued_cheat.len() + plan.restart.len() + plan.ignored.len(),
            seen,
            "every change lands in exactly one bucket"
        );
        assert!(!plan.ignored.is_empty(), "kaiz sets r_shadows and friends");
        assert_eq!(plan.denied.first().map(String::as_str), Some(DENIED));
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
    let base = resolve_base(&profile, no_cache()).unwrap();
    let applied = target(VANILLA, None, &base, &profile, catalog(), Extras::default()).unwrap();
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
        waiting_paks: WaitingPaks::None,
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
            receipt: report.receipt.clone(),
            hud_changed: false,
            addons_changed: false,
            paks_deferred: false,
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
        waiting_paks: WaitingPaks::None,
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
        waiting_paks: WaitingPaks::None,
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
    let base = resolve_base(&profile, no_cache()).unwrap();
    let tuned = target(&modded, None, &base, &profile, catalog(), Extras::default()).unwrap();
    let install = fake_install(&tuned.gameinfo, None);

    let safe = ranked_safe_target(&tuned.gameinfo, &install.store, None, None).unwrap();
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
    let safe = ranked_safe_target(&tuned.gameinfo, &install.store, None, None).unwrap();
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
    plan_with_hud_in(install, profile, ApplyContext::default())
}

fn plan_with_hud_in(install: &FakeInstall, profile: &Profile, ctx: ApplyContext) -> ApplyPlan {
    let live = read(&install.paths.gameinfo);
    let hud = hud_plan(&install.paths, &profile.hud, &install.store).unwrap();
    let base = resolve_base(profile, no_cache()).unwrap();
    let tgt = target(
        &live,
        None,
        &base,
        profile,
        catalog(),
        Extras {
            hud,
            ..Extras::default()
        },
    )
    .unwrap();
    plan(&install.paths, &live, None, &tgt, catalog(), ctx).unwrap()
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
    let safe = ranked_safe_target(&live, &install.store, hud, None).unwrap();
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

fn addons_profile(install: &FakeInstall) -> Profile {
    let upstream = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../research/configs/OptimizationLock/Various Addons Relating to Performance/Optimized Soul Container/pak01_dir.vpk",
    );
    let cache = dt_core::addons::sources::cache_dir(&install.store.root);
    dt_core::addons::sources::import(&cache, &upstream).unwrap();
    let mut profile = kaiz_profile();
    profile
        .addons
        .set_enabled(dt_core::addons::AddonId::SoulContainer, true);
    profile
}

#[test]
fn enabled_addon_installs_its_pak_mounts_addons_then_ranked_safe_removes_it() {
    use dt_core::addons::Action;
    let install = fake_install(VANILLA, None);
    let profile = addons_profile(&install);
    let first = plan_for(&install, &profile, ApplyContext::default());
    let addons = first.addons.as_ref().expect("addons plan");
    assert!(matches!(addons.addons[0].action, Action::Write(_)));
    assert!(addons.needs_search_path);
    assert_eq!(first.addon_changes(), 1);
    assert!(has_addons(&first.gameinfo.as_ref().unwrap().after).unwrap());

    let report = execute(&install.paths, &first, &install.store, None).unwrap();
    assert!(report.addons_changed && report.needs_restart && !report.hud_changed);
    let pak = addons_dir(&install.paths).join("pak75_dir.vpk");
    assert!(pak.is_file());
    assert!(has_addons(&read(&install.paths.gameinfo)).unwrap());

    let again = plan_for(&install, &profile, ApplyContext::default());
    assert!(
        again.is_empty(),
        "second apply has nothing to do: {again:?}"
    );
    assert_eq!(again.addon_changes(), 0);

    let live = read(&install.paths.gameinfo);
    let removal = addons_plan(&install.paths, &AddonsConfig::default(), &install.store).unwrap();
    assert_eq!(removal.as_ref().unwrap().addons[0].action, Action::Remove);
    let safe = ranked_safe_target(&live, &install.store, None, removal).unwrap();
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
    assert!(report.addons_changed);
    assert!(!pak.exists());
    assert_eq!(
        addons_plan(&install.paths, &AddonsConfig::default(), &install.store).unwrap(),
        None,
        "nothing enabled and nothing installed: no plan at all"
    );
}

#[test]
fn no_preset_writes_a_denylisted_value() {
    let stock = effective_values(VANILLA).unwrap();
    let presets = sidelock_cache();
    for p in preset::all() {
        let profile = Profile {
            base: BaseRef::Preset(p.id),
            convars: ConVarEdits::default(),
            ..kaiz_profile()
        };
        let base = resolve_base(&profile, presets.path()).unwrap();
        let tgt = target(VANILLA, None, &base, &profile, catalog(), Extras::default()).unwrap();
        for (name, value) in effective_values(&tgt.gameinfo).unwrap() {
            if catalog().is_denied(&name) {
                assert_eq!(
                    stock.get(&name),
                    Some(&value),
                    "{:?} writes denylisted {name}",
                    p.id
                );
            }
        }
    }
}

#[test]
fn practice_mode_writes_scene_system_keys_and_ranked_safe_restores_them() {
    let mut profile = kaiz_profile();
    profile.practice = PracticeMode {
        shadows: true,
        fog: false,
        batching: true,
    };
    let base = resolve_base(&profile, no_cache()).unwrap();
    let tuned = target(VANILLA, None, &base, &profile, catalog(), Extras::default()).unwrap();
    assert_eq!(practice::detect(&tuned.gameinfo).unwrap(), profile.practice);
    let install = fake_install(VANILLA, None);
    let plan_on = plan(
        &install.paths,
        VANILLA,
        None,
        &tuned,
        catalog(),
        ApplyContext::default(),
    )
    .unwrap();
    assert_eq!(plan_on.sections.len(), 10, "{:?}", plan_on.sections);
    assert!(
        plan_on
            .sections
            .iter()
            .all(|s| s.starts_with("SceneSystem/"))
    );
    assert!(
        plan_on
            .sections
            .contains(&"SceneSystem/LayerBatchThresholdFullsort".to_string())
    );
    let report = execute(&install.paths, &plan_on, &install.store, None).unwrap();
    assert!(report.needs_restart);
    let live = read(&install.paths.gameinfo);
    assert_eq!(practice::detect(&live).unwrap(), profile.practice);
    let record = Record::load(&install.store.root).unwrap();
    assert_eq!(
        record.prior("CSMCascadeResolution"),
        Some(Some("2048")),
        "execute saved what the keys held"
    );
    assert_eq!(record.prior("VolumetricFog"), None, "fog was never written");

    profile.practice = PracticeMode::default();
    let off = target(
        &live,
        None,
        &base,
        &profile,
        catalog(),
        Extras {
            practice: record.clone(),
            ..Extras::default()
        },
    )
    .unwrap();
    assert_eq!(
        outside_convars(&off.gameinfo),
        outside_convars(VANILLA),
        "off puts back what the record holds"
    );
    assert!(off.practice_record.as_ref().unwrap().is_empty());
    let plan_off = plan_for(&install, &profile, ApplyContext::default());
    execute(&install.paths, &plan_off, &install.store, None).unwrap();
    assert!(
        !install.store.root.join(practice::RECORD_FILE).exists(),
        "an empty record removes its file"
    );
    assert_eq!(
        outside_convars(&read(&install.paths.gameinfo)),
        outside_convars(VANILLA)
    );

    let safe = ranked_safe_target(&live, &install.store, None, None).unwrap();
    assert_eq!(safe.gameinfo, VANILLA, "ranked-safe is stock byte for byte");
    let plan_safe = plan(
        &install.paths,
        &live,
        None,
        &safe,
        catalog(),
        ApplyContext::default(),
    )
    .unwrap();
    assert_eq!(plan_safe.sections.len(), 10);
    let unchanged = plan(
        &install.paths,
        VANILLA,
        None,
        &ranked_safe_target(VANILLA, &install.store, None, None).unwrap(),
        catalog(),
        ApplyContext::default(),
    )
    .unwrap();
    assert!(unchanged.sections.is_empty() && unchanged.gameinfo.is_none());
}

/// A SideLock user's zeroed SceneSystem is theirs: a normal Apply never touches keys DeadTune
/// did not write, and the record only ever holds what DeadTune changed.
#[test]
fn apply_leaves_foreign_scene_system_edits_alone_unless_practice_wrote_them() {
    let foreign = VANILLA
        .replacen(
            "CSMCascadeResolution           \"2048\"",
            "CSMCascadeResolution           \"0\"",
            1,
        )
        .replacen(
            "VolumetricFog                     \"1\"",
            "VolumetricFog                     \"0\"",
            1,
        );
    let install = fake_install(&foreign, None);
    let mut profile = kaiz_profile();
    let plan_off = plan_for(&install, &profile, ApplyContext::default());
    assert!(plan_off.sections.is_empty(), "{:?}", plan_off.sections);
    execute(&install.paths, &plan_off, &install.store, None).unwrap();
    let after = read(&install.paths.gameinfo);
    assert_eq!(outside_convars(&after), outside_convars(&foreign));
    assert!(!install.store.root.join(practice::RECORD_FILE).exists());

    profile.practice = PracticeMode {
        shadows: true,
        ..PracticeMode::default()
    };
    let plan_on = plan_for(&install, &profile, ApplyContext::default());
    execute(&install.paths, &plan_on, &install.store, None).unwrap();
    let record = Record::load(&install.store.root).unwrap();
    assert_eq!(
        record.prior("CSMCascadeResolution"),
        None,
        "already the practice value: the player's own"
    );
    assert_eq!(record.prior("DynamicShadowResolution"), Some(Some("1")));

    profile.practice = PracticeMode::default();
    let plan_back = plan_for(&install, &profile, ApplyContext::default());
    execute(&install.paths, &plan_back, &install.store, None).unwrap();
    let back = read(&install.paths.gameinfo);
    assert_eq!(outside_convars(&back), outside_convars(&foreign));
    assert!(back.contains("CSMCascadeResolution           \"0\""));

    let safe = ranked_safe_target(&back, &install.store, None, None).unwrap();
    assert!(
        safe.gameinfo
            .contains("CSMCascadeResolution           \"2048\"")
    );
    assert!(
        safe.gameinfo
            .contains("VolumetricFog                     \"1\"")
    );
}

const RUNNING: ApplyContext = ApplyContext {
    in_sandbox: false,
    game_running: true,
    waiting_paks: WaitingPaks::None,
};

#[test]
fn a_running_game_defers_the_hud_pak_and_writes_everything_else() {
    let install = fake_install(VANILLA, None);
    with_game_pak(&install);
    let profile = hud_profile();
    let plan = plan_with_hud_in(&install, &profile, RUNNING);
    assert_eq!(plan.paks, PakTiming::AfterExit);

    let report = execute(&install.paths, &plan, &install.store, None).unwrap();
    assert!(report.paks_deferred && report.wrote_gameinfo && !report.hud_changed);
    let pak = addons_dir(&install.paths).join(ADDON_FILE);
    assert!(!pak.exists(), "the running game holds the addon paks");
    assert!(has_addons(&read(&install.paths.gameinfo)).unwrap());
    assert!(
        addons_dir(&install.paths).is_dir(),
        "the mounted dir exists before the pak"
    );

    let queued = plan_with_hud_in(
        &install,
        &profile,
        ApplyContext {
            waiting_paks: WaitingPaks::Same,
            ..RUNNING
        },
    );
    assert_eq!(queued.paks, PakTiming::Queued);
    assert!(
        queued.is_empty(),
        "already waiting for the game to close: {queued:?}"
    );

    let hud = hud_plan(&install.paths, &profile.hud, &install.store).unwrap();
    let written = write_paks(&install.paths, hud.as_ref(), None, &install.store).unwrap();
    assert!(written.hud_changed && !written.locked);
    assert!(pak.is_file());
    assert!(plan_with_hud(&install, &profile).is_empty());
}

#[test]
fn nothing_waits_when_the_paks_do_not_change() {
    let install = fake_install(VANILLA, None);
    with_game_pak(&install);
    let profile = hud_profile();
    execute(
        &install.paths,
        &plan_with_hud(&install, &profile),
        &install.store,
        None,
    )
    .unwrap();

    let mut convar_only = profile.clone();
    convar_only
        .convars
        .set
        .insert("fps_max".into(), "240".into());
    let plan = plan_with_hud_in(&install, &convar_only, RUNNING);
    let report = execute(&install.paths, &plan, &install.store, None).unwrap();
    assert!(report.wrote_gameinfo && !report.paks_deferred);
}

#[cfg(unix)]
#[test]
fn a_pak_the_game_holds_open_waits_instead_of_failing_the_apply() {
    use std::os::unix::fs::PermissionsExt;
    let install = fake_install(VANILLA, None);
    with_game_pak(&install);
    let dir = addons_dir(&install.paths);
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();

    let plan = plan_with_hud(&install, &hud_profile());
    assert_eq!(plan.paks, PakTiming::Now);
    let report = execute(&install.paths, &plan, &install.store, None);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    let report = report.unwrap();
    assert!(report.paks_deferred && report.wrote_gameinfo && !report.hud_changed);
    assert!(!dir.join(ADDON_FILE).exists());
}

#[test]
fn waiting_paks_survive_a_restart_and_clear() {
    let install = fake_install(VANILLA, None);
    let root = &install.store.root;
    assert_eq!(PendingPaks::load(root).unwrap(), None);
    let pending = PendingPaks {
        target: PakTarget {
            hud: hud_profile().hud,
            addons: AddonsConfig::default(),
        },
        discard_to: PakTarget::default(),
        kinds: PakKinds::Hud,
    };
    pending.save(root).unwrap();
    assert_eq!(PendingPaks::load(root).unwrap(), Some(pending));
    PendingPaks::clear(root).unwrap();
    PendingPaks::clear(root).unwrap();
    assert_eq!(PendingPaks::load(root).unwrap(), None);
}
