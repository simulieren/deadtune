//! Runs the built `deadtune-cli` against a tempdir fake install.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use dt_core::gi::effective_values;

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

const VANILLA: &str = "research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi";
const PROFILE: &str = r#"name = "laptop"
base = "kaiz_minspec"

[convars.set]
csm_max_visible_dist = "2000"
fps_max = "120"

[convars.comment]
names = ["citadel_damage_offscreen_indicator_disabled"]

[video]
"setting.fps_max" = "120"
"#;

struct Fake {
    tmp: tempfile::TempDir,
    game: PathBuf,
}

impl Fake {
    fn new() -> Fake {
        let tmp = tempfile::tempdir().unwrap();
        let steamapps = tmp.path().join("steamapps");
        let game = steamapps.join("common/Deadlock");
        let citadel = game.join("game/citadel");
        fs::create_dir_all(citadel.join("cfg")).unwrap();
        fs::copy(repo(VANILLA), citadel.join("gameinfo.gi")).unwrap();
        fs::copy(
            repo("research/configs/OptimizationLock/test_cfg/video.txt"),
            citadel.join("cfg/video.txt"),
        )
        .unwrap();
        fs::write(
            steamapps.join("appmanifest_1422450.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1422450\"\n\t\"buildid\"\t\t\"20261004\"\n}\n",
        )
        .unwrap();
        let hud = fs::read(repo("crates/dt-core/tests/fixtures/hud/hud_vanilla.vcss_c")).unwrap();
        let files = BTreeMap::from([("panorama/styles/hud.vcss_c".to_string(), hud)]);
        fs::write(
            citadel.join("pak01_dir.vpk"),
            dt_core::hud::vpk::write(&files),
        )
        .unwrap();
        fs::write(tmp.path().join("laptop.toml"), PROFILE).unwrap();
        Fake { tmp, game }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn gameinfo(&self) -> PathBuf {
        self.game.join("game/citadel/gameinfo.gi")
    }

    fn cfg(&self) -> PathBuf {
        self.game.join("game/citadel/cfg")
    }

    fn data(&self) -> PathBuf {
        self.tmp.path().join("data")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_deadtune-cli"))
            .arg("--game-dir")
            .arg(&self.game)
            .arg("--data-dir")
            .arg(self.data())
            .args(args)
            .current_dir(self.tmp.path())
            .stdin(Stdio::null())
            .env_remove("DEADTUNE_GAME_DIR")
            .output()
            .unwrap()
    }

    /// Runs and asserts the exit code, returning stdout.
    fn ok(&self, args: &[&str]) -> String {
        self.expect(args, 0)
    }

    fn expect(&self, args: &[&str], code: i32) -> String {
        let out = self.run(args);
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(
            out.status.code(),
            Some(code),
            "deadtune-cli {args:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        stdout + &stderr
    }

    fn backups(&self) -> usize {
        fs::read_dir(self.data().join("backups/gameinfo")).map_or(0, |d| d.count())
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn doctor_passes_on_the_fake_install_and_writes_nothing_into_the_game() {
    let fake = Fake::new();
    let before = read(&fake.gameinfo());
    let out = fake.ok(&["doctor"]);
    assert!(!out.contains("FAIL"), "{out}");
    for check in [
        "PASS  Find Deadlock",
        "PASS  Lossless edit",
        "PASS  Game build id            20261004",
        "PASS  Write cfg folder",
        "PASS  Game archive (HUD)",
        "PASS  Live console (netcon)",
        "No failures",
    ] {
        assert!(out.contains(check), "missing {check:?} in\n{out}");
    }
    assert_eq!(read(&fake.gameinfo()), before);
    let cfg: Vec<_> = fs::read_dir(fake.cfg()).unwrap().collect();
    assert_eq!(cfg.len(), 1, "only video.txt remains in cfg");
}

#[test]
fn doctor_fails_without_a_game() {
    let fake = Fake::new();
    fs::remove_file(fake.gameinfo()).unwrap();
    let out = fake.expect(&["doctor"], 1);
    assert!(out.contains("FAIL  Find Deadlock"), "{out}");
}

#[test]
fn help_and_usage_errors() {
    let fake = Fake::new();
    let top = fake.ok(&["--help"]);
    for name in [
        "doctor",
        "apply",
        "ranked-safe",
        "bench compare",
        "hud apply",
        "self-update",
    ] {
        assert!(top.contains(name), "{top}");
    }
    assert!(
        fake.ok(&["self-update", "--help"])
            .contains("--channel stable|testing")
    );
    assert!(
        fake.expect(&["self-update", "--channel", "beta"], 2)
            .contains("--channel is stable or testing")
    );
    assert!(fake.ok(&["apply", "--help"]).contains("--dry-run"));
    assert!(fake.ok(&["bench", "import", "-h"]).contains("--label"));
    assert!(fake.expect(&["frobnicate"], 2).contains("unknown command"));
    assert!(fake.expect(&["apply"], 2).contains("--profile is required"));
    assert!(
        fake.expect(&["diff", "--profile", "laptop.toml", "--bogus"], 2)
            .contains("--bogus")
    );
}

#[test]
fn diff_prints_plan_and_unified_diff_without_writing() {
    let fake = Fake::new();
    let before = read(&fake.gameinfo());
    let out = fake.ok(&["diff", "--profile", "laptop.toml"]);
    assert!(out.contains("gameinfo.gi: will be rewritten"), "{out}");
    assert!(out.contains("queued until sandbox (cheat)"), "{out}");
    assert!(out.contains("csm_max_visible_dist"), "{out}");
    assert!(out.contains("fps_max=120"), "live class pushed now: {out}");
    assert!(out.contains("setting.fps_max=120"), "{out}");
    assert!(out.contains("+++ "), "unified diff: {out}");
    assert_eq!(read(&fake.gameinfo()), before);

    let sandbox = fake.ok(&["diff", "--profile", "laptop.toml", "--sandbox"]);
    assert!(sandbox.contains("csm_max_visible_dist=2000"), "{sandbox}");
}

#[test]
fn practice_mode_edits_the_profile_applies_and_ranked_safe_restores() {
    let fake = Fake::new();
    let stock = read(&fake.gameinfo());
    let out = fake.ok(&["practice", "on", "--fog", "--profile", "laptop.toml"]);
    assert!(out.contains("shadows off, fog on, batching off"), "{out}");
    assert!(out.contains("refuse to find matches"), "{out}");
    assert!(read(&fake.file("laptop.toml")).contains("[practice]\n"));
    assert!(
        fake.ok(&["profile", "show", "laptop.toml"])
            .contains("practice shadows off, fog on, batching off")
    );

    let diff = fake.ok(&["diff", "--profile", "laptop.toml"]);
    assert!(diff.contains("practice mode"), "{diff}");
    assert!(diff.contains("SceneSystem/VolumetricFog"), "{diff}");
    fake.ok(&["apply", "--profile", "laptop.toml", "--yes"]);
    let applied = read(&fake.gameinfo());
    assert!(
        applied.contains("VolumetricFog                     \"0\""),
        "{applied}"
    );
    assert!(applied.contains("CSMCascadeResolution           \"2048\""));

    let out = fake.ok(&["ranked-safe", "--yes"]);
    assert!(out.contains("Wrote gameinfo.gi"), "{out}");
    assert_eq!(read(&fake.gameinfo()), stock, "stock byte for byte");

    let out = fake.ok(&["practice", "off", "--profile", "laptop.toml"]);
    assert!(out.contains("shadows off, fog off, batching off"), "{out}");
    assert!(!read(&fake.file("laptop.toml")).contains("practice"));
    let out = fake.expect(&["practice", "maybe", "--profile", "laptop.toml"], 2);
    assert!(out.contains("expected on or off"), "{out}");
}

#[test]
fn apply_needs_yes_without_a_terminal() {
    let fake = Fake::new();
    let before = read(&fake.gameinfo());
    let out = fake.expect(&["apply", "--profile", "laptop.toml"], 2);
    assert!(out.contains("--yes"), "{out}");
    fake.ok(&["apply", "--profile", "laptop.toml", "--dry-run"]);
    assert_eq!(read(&fake.gameinfo()), before);
    assert_eq!(fake.backups(), 0);
}

#[test]
fn apply_writes_then_second_apply_is_a_no_op() {
    let fake = Fake::new();
    let vanilla = read(&fake.gameinfo());
    let out = fake.ok(&[
        "apply",
        "--profile",
        "laptop.toml",
        "--yes",
        "--bridge",
        "execfile",
        "--sandbox",
    ]);
    assert!(out.contains("Wrote gameinfo.gi, video.txt"), "{out}");
    assert!(out.contains("Pushed"), "{out}");
    let applied = read(&fake.gameinfo());
    assert_eq!(effective_values(&applied).unwrap()["fps_max"], "120");
    assert!(read(&fake.cfg().join("video.txt")).contains("\"setting.fps_max\"\t\t\"120\""));
    assert!(read(&fake.cfg().join("deadtune_live.cfg")).contains("csm_max_visible_dist \"2000\""));
    assert_eq!(read(&fake.data().join("original/gameinfo.gi")), vanilla);
    assert_eq!(fake.backups(), 1);

    let again = fake.ok(&["apply", "--profile", "laptop.toml", "--yes"]);
    assert!(again.contains("No changes"), "{again}");
    assert_eq!(read(&fake.gameinfo()), applied);
    assert_eq!(fake.backups(), 1);

    let status = fake.ok(&["status", "--profile", "laptop.toml"]);
    assert!(
        status.contains("live gameinfo differs from the original"),
        "{status}"
    );
    assert!(status.contains("No changes"), "{status}");
}

#[test]
fn ranked_safe_restores_stock_block_and_keeps_search_paths() {
    let fake = Fake::new();
    let modded = read(&fake.gameinfo()).replacen(
        "Game_UILanguage",
        "Game citadel/mymod\n            Game_UILanguage",
        1,
    );
    fs::write(fake.gameinfo(), &modded).unwrap();
    fake.ok(&["apply", "--profile", "laptop.toml", "--yes"]);
    assert_ne!(read(&fake.gameinfo()), modded);

    let out = fake.ok(&["ranked-safe", "--yes"]);
    assert!(out.contains("Wrote gameinfo.gi"), "{out}");
    let safe = read(&fake.gameinfo());
    assert_eq!(
        effective_values(&safe).unwrap(),
        effective_values(&modded).unwrap()
    );
    assert!(safe.contains("Game citadel/mymod"));
    assert!(fake.ok(&["ranked-safe", "--yes"]).contains("No changes"));
}

#[test]
fn restore_original_and_list() {
    let fake = Fake::new();
    let vanilla = read(&fake.gameinfo());
    fake.ok(&["apply", "--profile", "laptop.toml", "--yes"]);
    let list = fake.ok(&["restore", "--list"]);
    assert!(
        list.contains("gameinfo.gi original") && list.contains("gameinfo.gi backup"),
        "{list}"
    );
    fake.ok(&["restore", "--original", "--kind", "gameinfo"]);
    assert_eq!(read(&fake.gameinfo()), vanilla);
    assert_eq!(fake.backups(), 2, "restore backs up the file it replaces");
    fake.ok(&["restore", "--latest", "--kind", "gameinfo"]);
    assert_ne!(
        read(&fake.gameinfo()),
        vanilla,
        "latest backup is the applied file"
    );
    assert!(fake.expect(&["restore"], 2).contains("exactly one"));
}

#[test]
fn push_writes_exec_file_prints_clipboard_and_refuses_denylist() {
    let fake = Fake::new();
    let out = fake.ok(&["push", "fps_max=60", "r_farz=6000"]);
    assert!(out.contains("exec deadtune_live"), "{out}");
    let cfg = read(&fake.cfg().join("deadtune_live.cfg"));
    assert!(
        cfg.contains("fps_max \"60\"") && cfg.contains("r_farz \"6000\""),
        "{cfg}"
    );

    let clip = fake.ok(&["push", "fps_max=60", "cl_x=1", "--bridge", "clipboard"]);
    assert!(clip.contains("fps_max \"60\"; cl_x \"1\""), "{clip}");
    assert!(
        fake.expect(&["push", "citadel_player_outline_enemies=1"], 1)
            .contains("denylist")
    );
    assert!(fake.expect(&["push", "fps_max"], 2).contains("name=value"));
    assert!(
        cfg.contains("echo DEADTUNE_ACK ") && cfg.contains("echo DEADTUNE_END "),
        "ack trailer: {cfg}"
    );
}

#[test]
fn push_wait_reports_each_convar_from_the_console_log_or_times_out() {
    let fake = Fake::new();
    let out = fake.expect(&["push", "fps_max=60", "--wait", "1"], 1);
    assert!(out.contains("no reply from Deadlock after 1s"), "{out}");
    assert!(
        out.contains("console.log"),
        "names the files it looked in: {out}"
    );

    let live = fake.cfg().join("deadtune_live.cfg");
    fs::remove_file(&live).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_deadtune-cli"))
        .arg("--game-dir")
        .arg(&fake.game)
        .arg("--data-dir")
        .arg(fake.data())
        .args(["push", "fps_max=60", "r_devonly=1", "--wait", "20"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("DEADTUNE_GAME_DIR")
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let nonce = loop {
        if let Ok(text) = fs::read_to_string(&live)
            && let Some(rest) = text
                .lines()
                .find_map(|l| l.strip_prefix("echo DEADTUNE_ACK "))
        {
            break rest.split(' ').next().unwrap().to_string();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "live cfg never written"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    fs::write(
        fake.game.join("game/citadel/console.log"),
        format!(
            "DEADTUNE_ACK {nonce} 2\r\n\"fps_max\" = \"60\" ( def. \"400\" )\r\nUnknown command 'r_devonly'\r\nDEADTUNE_END {nonce}\r\n"
        ),
    )
    .unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("fps_max = 60"), "{stdout}");
    assert!(
        stdout.contains("r_devonly: rejected (Unknown command 'r_devonly')"),
        "{stdout}"
    );
    assert!(stdout.contains("Deadlock applied 1 of 2."), "{stdout}");
}

#[test]
fn bench_import_list_compare() {
    let fake = Fake::new();
    let csv = |name: &str| repo(&format!("crates/dt-core/testdata/bench/{name}"));
    let a = csv("presentmon_2x_current.csv");
    let b = csv("mangohud.csv");
    fake.ok(&[
        "bench",
        "import",
        a.to_str().unwrap(),
        "--profile",
        "laptop",
        "--label",
        "before",
    ]);
    fake.ok(&[
        "bench",
        "import",
        b.to_str().unwrap(),
        "--profile",
        "laptop",
        "--label",
        "after",
    ]);
    let list = fake.ok(&["bench", "list", "--profile", "laptop"]);
    assert!(
        list.contains("before") && list.contains("after") && list.contains("1% low"),
        "{list}"
    );
    let cmp = fake.ok(&["bench", "compare", "laptop", "before", "after"]);
    assert!(cmp.contains("after vs before: avg"), "{cmp}");
    assert!(
        fake.expect(&["bench", "compare", "laptop", "before", "nope"], 1)
            .contains("nope")
    );
}

#[test]
fn profile_new_show_and_overrides_round_trip() {
    let fake = Fake::new();
    fake.ok(&["profile", "new", "fresh.toml", "--base", "optilock_potato"]);
    assert!(read(&fake.file("fresh.toml")).contains("base = \"optilock_potato\""));
    assert!(
        fake.expect(&["profile", "new", "fresh.toml", "--base", "vanilla"], 1)
            .contains("exists")
    );
    assert!(
        fake.expect(&["profile", "new", "x.toml", "--base", "nope"], 2)
            .contains("unknown base")
    );

    let example = repo("research/configs/OptimizationLock/auto updater/overrides.example.gi");
    fake.ok(&[
        "profile",
        "import-overrides",
        example.to_str().unwrap(),
        "imported.toml",
        "--base",
        "sqooky",
    ]);
    fake.ok(&["profile", "export-overrides", "imported.toml", "out.gi"]);
    let reimported = dt_core::profile::parse_overrides_gi(&read(&fake.file("out.gi"))).unwrap();
    let original = dt_core::profile::parse_overrides_gi(&read(&example)).unwrap();
    assert_eq!(reimported, original);
    let show = fake.ok(&["profile", "show", "laptop.toml"]);
    assert!(
        show.contains("base: kaiz_minspec") && show.contains("set      fps_max = 120"),
        "{show}"
    );
}

#[test]
fn hud_apply_status_remove() {
    let fake = Fake::new();
    fs::write(
        fake.file("hud.toml"),
        "[elements.minimap]\nscale_pct = 120\n",
    )
    .unwrap();
    let addon = fake.game.join("game/citadel/addons/pak77_dir.vpk");
    let out = fake.ok(&["hud", "apply", "--layout", "hud.toml", "--yes"]);
    assert!(out.contains("adds `Game citadel/addons`"), "{out}");
    assert!(addon.is_file());
    assert!(read(&fake.gameinfo()).contains("citadel/addons"));
    let status = fake.ok(&["hud", "status"]);
    assert!(status.contains("HUD addon: installed, current"), "{status}");
    assert!(fake.ok(&["status"]).contains("HUD addon: installed"));

    fake.ok(&["hud", "remove", "--yes"]);
    assert!(!addon.exists());
    assert!(
        fake.ok(&["hud", "status"])
            .contains("HUD addon: not installed")
    );
    assert!(
        fake.ok(&["hud", "remove", "--yes"])
            .contains("nothing to do")
    );
}

#[test]
fn watch_reports_overwrites_and_game_updates() {
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let fake = Fake::new();
    fs::write(fake.file("hud.toml"), "[elements.chat]\nopacity_pct = 50\n").unwrap();
    fake.ok(&["hud", "apply", "--layout", "hud.toml", "--yes"]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_deadtune-cli"))
        .arg("--game-dir")
        .arg(&fake.game)
        .arg("--data-dir")
        .arg(fake.data())
        .arg("watch")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    // Pokes repeat because the watcher may not be armed yet when the first write lands.
    let wait_for = |needle: &str, poke: &dyn Fn()| {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut next_poke = Instant::now();
        while Instant::now() < deadline {
            if Instant::now() >= next_poke {
                poke();
                next_poke = Instant::now() + Duration::from_millis(500);
            }
            if let Ok(line) = rx.recv_timeout(Duration::from_millis(100))
                && line.contains(needle)
            {
                return true;
            }
        }
        false
    };
    let gameinfo = fake.gameinfo();
    let saw_overwrite = wait_for("gameinfo.gi changed", &|| {
        fs::write(&gameinfo, read(&gameinfo) + "\n").unwrap();
    });
    let manifest = fake.tmp.path().join("steamapps/appmanifest_1422450.acf");
    let saw_update = wait_for("game updated: build 20261004 -> 20261005", &|| {
        fs::write(
            &manifest,
            "\"AppState\"\n{\n\t\"buildid\"\t\t\"20261005\"\n}\n",
        )
        .unwrap();
    });
    let saw_stale = wait_for("HUD addon was built for the old build", &|| {});
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(saw_overwrite, "watch reports a gameinfo overwrite");
    assert!(saw_update, "watch reports a buildid change");
    assert!(
        saw_stale,
        "watch flags the HUD addon as stale after an update"
    );
}
