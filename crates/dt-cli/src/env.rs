//! Where the game and our data live, plus helpers several commands share.

use std::io::{BufRead, IsTerminal, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use dt_core::apply::{self, ApplyContext, ApplyPlan};
use dt_core::backup::{self, BackupStore};
use dt_core::bridge::Bridge;
use dt_core::bridge::execfile::ExecFileBridge;
use dt_core::bridge::netcon::{self, NetconBridge};
use dt_core::catalog::Catalog;
use dt_core::hud::install::HudAction;
use dt_core::locate::{self, GamePaths};
use dt_core::preset;
use dt_core::profile::{BaseRef, Profile};

use crate::args::{Args, CliError, fail, usage};

pub const GAME_DIR_ENV: &str = "DEADTUNE_GAME_DIR";

pub struct Env {
    pub game_dir: Option<PathBuf>,
    pub data_dir: PathBuf,
}

impl Env {
    /// `--game-dir`, else `$DEADTUNE_GAME_DIR`, else Steam lookup.
    pub fn paths(&self) -> Result<GamePaths, CliError> {
        let dir = self
            .game_dir
            .clone()
            .or_else(|| std::env::var_os(GAME_DIR_ENV).map(PathBuf::from));
        match dir {
            Some(dir) => Ok(locate::from_game_root(&dir)?),
            None => Ok(locate::locate()?),
        }
    }

    pub fn store(&self) -> Result<BackupStore, CliError> {
        Ok(BackupStore::open(&self.data_dir)?)
    }

    pub fn bench_dir(&self) -> PathBuf {
        self.data_dir.join("bench")
    }
}

pub fn default_data_dir() -> PathBuf {
    backup::data_dir()
}

pub fn read(path: &Path) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(|e| fail(format!("{}: {e}", path.display())))
}

pub fn read_opt(path: &Path) -> Result<Option<String>, CliError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(fail(format!("{}: {e}", path.display()))),
    }
}

pub fn load_profile(path: &Path) -> Result<Profile, CliError> {
    Profile::from_toml(&read(path)?).map_err(|e| fail(format!("{}: {e}", path.display())))
}

pub fn save_new(path: &Path, text: &str) -> Result<(), CliError> {
    if path.exists() {
        return Err(fail(format!("{} already exists", path.display())));
    }
    std::fs::write(path, text).map_err(|e| fail(format!("{}: {e}", path.display())))
}

/// `kaiz_minspec` or `file:<path>`.
pub fn parse_base(text: &str) -> Result<BaseRef, CliError> {
    if let Some(path) = text.strip_prefix("file:") {
        return Ok(BaseRef::File(PathBuf::from(path)));
    }
    preset::all()
        .iter()
        .find(|p| p.id.key() == text)
        .map(|p| BaseRef::Preset(p.id))
        .ok_or_else(|| {
            let keys: Vec<_> = preset::all().iter().map(|p| p.id.key()).collect();
            usage(format!(
                "unknown base {text:?}; expected file:<path> or one of {}",
                keys.join(", ")
            ))
        })
}

pub fn base_label(base: &BaseRef) -> String {
    match base {
        BaseRef::Preset(id) => id.key().to_string(),
        BaseRef::File(path) => format!("file:{}", path.display()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeChoice {
    None,
    ExecFile,
    Netcon(u16),
    Clipboard,
}

pub fn parse_bridge(
    text: &str,
    allow_none: bool,
    allow_clipboard: bool,
) -> Result<BridgeChoice, CliError> {
    let choice = match text.split_once(':') {
        Some(("netcon", port)) => BridgeChoice::Netcon(
            port.parse()
                .map_err(|_| usage(format!("bad netcon port {port:?}")))?,
        ),
        _ => match text {
            "none" if allow_none => BridgeChoice::None,
            "execfile" => BridgeChoice::ExecFile,
            "netcon" => BridgeChoice::Netcon(netcon::DEFAULT_PORT),
            "clipboard" if allow_clipboard => BridgeChoice::Clipboard,
            _ => return Err(usage(format!("unknown bridge {text:?}"))),
        },
    };
    Ok(choice)
}

pub fn open_bridge(
    choice: BridgeChoice,
    paths: &GamePaths,
) -> Result<Option<Box<dyn Bridge>>, CliError> {
    Ok(match choice {
        BridgeChoice::None | BridgeChoice::Clipboard => None,
        BridgeChoice::ExecFile => Some(Box::new(ExecFileBridge {
            cfg_dir: paths.cfg_dir.clone(),
        })),
        BridgeChoice::Netcon(port) => Some(Box::new(
            NetconBridge::connect(
                SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                Duration::from_secs(2),
            )
            .map_err(|e| {
                fail(format!(
                    "netcon 127.0.0.1:{port}: {e} (launch with -netconport {port})"
                ))
            })?,
        )),
    })
}

/// Everything `apply --profile` would do, without doing it.
pub fn profile_plan(
    env: &Env,
    paths: &GamePaths,
    profile: &Profile,
    in_sandbox: bool,
) -> Result<ApplyPlan, CliError> {
    let store = env.store()?;
    let live = read(&paths.gameinfo)?;
    let live_video = read_opt(&paths.video)?;
    let catalog = Catalog::embedded();
    let base = apply::resolve_base(profile)?;
    let hud = apply::hud_plan(paths, &profile.hud, &store)?;
    let addons = apply::addons_plan(paths, &profile.addons, &store)?;
    let target = apply::target(
        &live,
        live_video.as_deref(),
        &base,
        profile,
        catalog,
        hud,
        addons,
    )?;
    let ctx = ApplyContext {
        in_sandbox,
        game_running: dt_core::launch::is_game_running(),
    };
    Ok(apply::plan(
        paths,
        &live,
        live_video.as_deref(),
        &target,
        catalog,
        ctx,
    )?)
}

pub fn print_plan(plan: &ApplyPlan, diffs: bool) {
    if plan.is_empty() {
        println!("No changes: the files already match.");
    }
    for (label, write) in [("gameinfo.gi", &plan.gameinfo), ("video.txt", &plan.video)] {
        match write {
            Some(w) => println!("{label}: will be rewritten ({})", w.path.display()),
            None => println!("{label}: unchanged"),
        }
    }
    let live: Vec<String> = plan
        .live
        .iter()
        .map(|c| format!("{}={}", c.name, c.value))
        .collect();
    let video: Vec<String> = plan
        .video_changes
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    print_list("live now", &live);
    print_list("queued until sandbox (cheat)", &plan.queued_cheat);
    print_list("next launch", &plan.restart);
    print_list("video settings", &video);
    print_list("refused (denylist)", &plan.denied);
    if let Some(hud) = &plan.hud {
        let action = match &hud.action {
            HudAction::Write(bytes) => {
                format!("write {} ({} bytes)", hud.addon_path.display(), bytes.len())
            }
            HudAction::Remove => format!("remove {}", hud.addon_path.display()),
            HudAction::Nothing => "unchanged".to_string(),
        };
        println!("HUD addon: {action}");
        if hud.needs_search_path {
            println!("HUD addon: adds `Game citadel/addons` to SearchPaths");
        }
        for c in &hud.conflicts {
            println!(
                "HUD conflict: {} also overrides {}",
                c.addon.display(),
                c.paths.join(", ")
            );
        }
    }
    if let Some(addons) = &plan.addons {
        for a in &addons.addons {
            println!(
                "addon {}: {} {}",
                a.id.key(),
                crate::cmd_addons::describe_action(&a.action),
                a.path.display()
            );
        }
        if addons.needs_search_path {
            println!("addons: adds `Game citadel/addons` to SearchPaths");
        }
        for c in &addons.conflicts {
            println!(
                "addon conflict: {} also overrides {} ({})",
                c.addon.display(),
                c.paths.join(", "),
                c.id.key()
            );
        }
    }
    if diffs {
        for write in [&plan.gameinfo, &plan.video].into_iter().flatten() {
            print!("{}", write.unified_diff());
        }
    }
}

fn print_list(label: &str, items: &[String]) {
    if !items.is_empty() {
        println!("{label} ({}): {}", items.len(), items.join(", "));
    }
}

/// `--yes` skips the question. Without it, ask on a terminal and refuse otherwise.
pub fn confirm(args: &Args, question: &str) -> Result<bool, CliError> {
    if args.switch("yes") {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err(usage("stdin is not a terminal; pass --yes to confirm"));
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}
