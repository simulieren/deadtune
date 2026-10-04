//! locate, status, presets, catalog, doctor

use std::path::{Path, PathBuf};

use dt_core::backup::{FileKind, sha256_hex};
use dt_core::bridge::netcon::{DEFAULT_PORT, NetconBridge};
use dt_core::catalog::Catalog;
use dt_core::gi::{self, Eol, Overrides};
use dt_core::locate::{GamePaths, parse_buildid};
use dt_core::{launch, power, preset, video, watch};

use crate::args::{Args, CliResult, fail};
use crate::cmd_hud;
use crate::env::{self, Env};

fn buildid(paths: &GamePaths) -> Option<String> {
    let acf = std::fs::read_to_string(paths.appmanifest.as_ref()?).ok()?;
    parse_buildid(&acf)
}

fn opt_path(p: &Option<PathBuf>) -> String {
    p.as_ref()
        .map_or("(not found)".into(), |p| p.display().to_string())
}

pub fn locate(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    println!("game root:   {}", paths.game_root.display());
    println!("gameinfo:    {}", paths.gameinfo.display());
    println!("video:       {}", paths.video.display());
    println!("cfg dir:     {}", paths.cfg_dir.display());
    println!("appmanifest: {}", opt_path(&paths.appmanifest));
    println!("steam root:  {}", opt_path(&paths.steam_root));
    println!(
        "buildid:     {}",
        buildid(&paths).unwrap_or_else(|| "(unknown)".into())
    );
    println!("data dir:    {}", env.data_dir.display());
    Ok(())
}

pub fn status(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let store = env.store()?;
    let fp = watch::fingerprint(&paths);
    let short = |sha: &Option<String>| {
        sha.as_deref()
            .map_or("(missing)".into(), |s| s[..12].to_string())
    };
    println!(
        "game running: {}",
        if launch::is_game_running() {
            "yes"
        } else {
            "no"
        }
    );
    println!("power source: {:?}", power::power_source());
    println!(
        "buildid:      {}",
        fp.buildid.as_deref().unwrap_or("(unknown)")
    );
    println!("gameinfo:     sha256 {}", short(&fp.gameinfo_sha));
    println!("video:        sha256 {}", short(&fp.video_sha));
    let original = match store.original(FileKind::GameInfo) {
        None => "no original snapshot yet (taken on first apply)".to_string(),
        Some(o) if Some(&o.sha256) == fp.gameinfo_sha.as_ref() => {
            "live gameinfo matches the original".into()
        }
        Some(_) => "live gameinfo differs from the original".into(),
    };
    println!("original:     {original}");
    for line in cmd_hud::report_lines(env, &paths) {
        println!("{line}");
    }
    if let Some(file) = args.value("profile") {
        let profile = env::load_profile(Path::new(file))?;
        println!("pending for profile {:?}:", profile.name);
        let plan = env::profile_plan(env, &paths, &profile, args.switch("sandbox"))?;
        env::print_plan(&plan, false);
    }
    Ok(())
}

pub fn presets(_: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    for p in preset::all() {
        let video = if p.pinned_video.is_some() {
            "  +video.txt"
        } else {
            ""
        };
        println!("{:<22} {:<28} by {}{video}", p.id.key(), p.label, p.author);
    }
    Ok(())
}

pub fn catalog(_: &Env, args: &Args) -> CliResult {
    let query = args.pos.join(" ");
    let mut n = 0;
    for (name, e) in Catalog::embedded().search(&query) {
        n += 1;
        let deny = if e.denylist { "  DENYLIST" } else { "" };
        println!(
            "{name:<48} {:<10} impact={:<7} default={:<10} {}{deny}",
            format!("{:?}", e.apply),
            format!("{:?}", e.impact),
            e.default.as_deref().unwrap_or("?"),
            e.category,
        );
    }
    println!("{n} convar(s)");
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    Info,
}

struct Doctor {
    failed: bool,
}

impl Doctor {
    fn line(&mut self, status: Status, check: &str, detail: impl AsRef<str>) {
        let tag = match status {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Info => "INFO",
        };
        self.failed |= status == Status::Fail;
        println!("{tag}  {check:<26} {}", detail.as_ref());
    }

    fn result<T>(
        &mut self,
        check: &str,
        result: Result<T, String>,
        ok: impl FnOnce(&T) -> String,
    ) -> Option<T> {
        match result {
            Ok(value) => {
                self.line(Status::Pass, check, ok(&value));
                Some(value)
            }
            Err(e) => {
                self.line(Status::Fail, check, e);
                None
            }
        }
    }
}

/// Writes and deletes a scratch file in `dir`.
fn probe_writable(dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let probe = dir.join(".deadtune_doctor.tmp");
    std::fs::write(&probe, b"deadtune").map_err(|e| format!("write: {e}"))?;
    std::fs::remove_file(&probe).map_err(|e| format!("delete: {e}"))?;
    Ok(dir.to_path_buf())
}

/// Read-only self test for a new machine. The only writes are scratch files it deletes again.
pub fn doctor(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let mut d = Doctor { failed: false };
    println!(
        "deadtune-cli {} doctor on {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );

    let paths = d.result("locate", env.paths().map_err(|e| e.message()), |p| {
        p.game_root.display().to_string()
    });
    if let Some(paths) = &paths {
        game_checks(&mut d, env, paths);
    }

    d.result("data dir writable", probe_writable(&env.data_dir), |p| {
        p.display().to_string()
    });
    d.result(
        "backups dir",
        env.store()
            .map_err(|_| "cannot open".to_string())
            .and_then(|s| {
                let n = s.list(FileKind::GameInfo).map_err(|e| e.to_string())?.len();
                let original = s.original(FileKind::GameInfo).is_some();
                Ok(format!(
                    "{}  original={}  gameinfo backups={n}",
                    s.root.display(),
                    if original { "yes" } else { "no" }
                ))
            }),
        |s| s.clone(),
    );
    let netcon = if NetconBridge::probe(DEFAULT_PORT) {
        format!("something is listening on 127.0.0.1:{DEFAULT_PORT}")
    } else {
        format!(
            "nothing on 127.0.0.1:{DEFAULT_PORT} (needs -netconport {DEFAULT_PORT}; exec-file bridge works without it)"
        )
    };
    d.line(Status::Info, "netcon probe", netcon);
    d.line(
        Status::Info,
        "game running",
        if launch::is_game_running() {
            "yes"
        } else {
            "no"
        },
    );

    if d.failed {
        Err(fail("doctor found failures"))
    } else {
        println!("All checks passed.");
        Ok(())
    }
}

fn game_checks(d: &mut Doctor, env: &Env, paths: &GamePaths) {
    let gameinfo = d.result(
        "gameinfo readable",
        std::fs::read_to_string(&paths.gameinfo).map_err(|e| e.to_string()),
        |t| format!("{} bytes", t.len()),
    );
    if let Some(text) = &gameinfo {
        let eol = match gi::detect_eol(text) {
            Eol::CrLf => "CRLF",
            Eol::Lf => "LF",
        };
        d.line(Status::Pass, "detect_eol", eol);
        d.result(
            "validate_braces",
            gi::validate_braces(text).map_err(|e| e.to_string()),
            |_| "balanced".into(),
        );
        d.result(
            "no-op apply is lossless",
            gi::apply_overrides(text, &Overrides::new())
                .map_err(|e| e.to_string())
                .and_then(|o| {
                    if o.text == *text {
                        Ok(())
                    } else {
                        Err("apply_overrides(empty) changed bytes".into())
                    }
                }),
            |_| "byte-identical".into(),
        );
        d.result(
            "read_convars",
            gi::read_convars(text).map_err(|e| e.to_string()),
            |v| {
                format!(
                    "{} convars, {} active",
                    v.len(),
                    v.iter().filter(|e| !e.commented).count()
                )
            },
        );
    }
    match std::fs::read_to_string(&paths.video) {
        Ok(text) => {
            d.line(
                Status::Pass,
                "video readable",
                format!("{} bytes", text.len()),
            );
            d.result(
                "video read_settings",
                video::read_settings(&text).map_err(|e| e.to_string()),
                |s| format!("{} settings", s.len()),
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => d.line(
            Status::Info,
            "video readable",
            "no video.txt yet (the game writes it on first launch)",
        ),
        Err(e) => d.line(Status::Fail, "video readable", e.to_string()),
    }
    d.result(
        "appmanifest buildid",
        buildid(paths).ok_or_else(|| format!("no buildid in {}", opt_path(&paths.appmanifest))),
        |b| b.clone(),
    );
    d.result("cfg dir writable", probe_writable(&paths.cfg_dir), |p| {
        p.display().to_string()
    });
    match cmd_hud::game_pak_check(paths) {
        Ok(Some(detail)) => d.line(Status::Pass, "game pak", detail),
        Ok(None) => d.line(
            Status::Info,
            "game pak",
            "no pak01_dir.vpk; HUD editing unavailable",
        ),
        Err(e) => d.line(Status::Fail, "game pak", e),
    }
    for line in cmd_hud::report_lines(env, paths) {
        let (check, detail) = line.split_once(": ").unwrap_or(("HUD", &line));
        d.line(Status::Info, check, detail);
    }
    if let Ok(bytes) = std::fs::read(&paths.gameinfo) {
        d.line(Status::Info, "gameinfo sha256", &sha256_hex(&bytes)[..16]);
    }
}
