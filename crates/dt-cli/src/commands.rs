//! The command table: names, usage, declared flags and handlers.

use crate::args::{Args, CliResult};
use crate::env::Env;
use crate::{
    cmd_addons, cmd_apply, cmd_bench, cmd_game, cmd_hud, cmd_info, cmd_presets, cmd_profile,
    cmd_snapshot, cmd_update,
};

pub struct Command {
    pub name: &'static str,
    pub usage: &'static str,
    pub summary: &'static str,
    pub values: &'static [&'static str],
    pub switches: &'static [&'static str],
    pub run: fn(&Env, &Args) -> CliResult,
}

const BRIDGE_HELP: &str = "--bridge none|execfile|netcon[:port]";

pub const COMMANDS: &[Command] = &[
    Command {
        name: "doctor",
        usage: "doctor [--report]",
        summary: "self test for a new machine; --report prints the plain-text diagnostic report",
        values: &[],
        switches: &["report"],
        run: cmd_info::doctor,
    },
    Command {
        name: "locate",
        usage: "locate",
        summary: "print game paths and buildid",
        values: &[],
        switches: &[],
        run: cmd_info::locate,
    },
    Command {
        name: "status",
        usage: "status [--profile <file>] [--sandbox]",
        summary: "game, power, file state, HUD addon, pending changes",
        values: &["profile"],
        switches: &["sandbox"],
        run: cmd_info::status,
    },
    Command {
        name: "presets",
        usage: "presets",
        summary: "list known presets, their authors, and where remote ones come from",
        values: &[],
        switches: &[],
        run: cmd_presets::list,
    },
    Command {
        name: "presets fetch",
        usage: "presets fetch <preset>",
        summary: "download a remote preset (SideLock) from GameBanana (fetch feature)",
        values: &[],
        switches: &[],
        run: cmd_presets::fetch,
    },
    Command {
        name: "presets import",
        usage: "presets import <preset> <cfg.zip or gameinfo.gi>",
        summary: "take a remote preset's downloaded file into DeadTune's cache",
        values: &[],
        switches: &[],
        run: cmd_presets::import,
    },
    Command {
        name: "presets accept",
        usage: "presets accept <preset>",
        summary: "use a remote preset file that differs from the one DeadTune checked",
        values: &[],
        switches: &[],
        run: cmd_presets::accept,
    },
    Command {
        name: "presets discard",
        usage: "presets discard <preset>",
        summary: "drop a remote preset file waiting for review",
        values: &[],
        switches: &[],
        run: cmd_presets::discard,
    },
    Command {
        name: "catalog",
        usage: "catalog [query]",
        summary: "search convars: apply class, impact, default",
        values: &[],
        switches: &[],
        run: cmd_info::catalog,
    },
    Command {
        name: "profile new",
        usage: "profile new <file> --base <preset|file:path>",
        summary: "create a profile",
        values: &["base"],
        switches: &[],
        run: cmd_profile::new,
    },
    Command {
        name: "profile show",
        usage: "profile show <file>",
        summary: "print a profile with catalog notes",
        values: &[],
        switches: &[],
        run: cmd_profile::show,
    },
    Command {
        name: "profile import-overrides",
        usage: "profile import-overrides <overrides.gi> <file> --base <preset|file:path>",
        summary: "create a profile from Sqooky's overrides.gi",
        values: &["base"],
        switches: &[],
        run: cmd_profile::import_overrides,
    },
    Command {
        name: "profile export-overrides",
        usage: "profile export-overrides <file> <overrides.gi>",
        summary: "write a profile's convar edits as overrides.gi",
        values: &[],
        switches: &[],
        run: cmd_profile::export_overrides,
    },
    Command {
        name: "diff",
        usage: "diff --profile <file> [--sandbox]",
        summary: "show what apply would do; writes nothing",
        values: &["profile"],
        switches: &["sandbox"],
        run: cmd_apply::diff,
    },
    Command {
        name: "apply",
        usage: "apply --profile <file> [--sandbox] [--bridge none|execfile|netcon[:port]] [--dry-run] [--yes]",
        summary: "write the profile and push live changes",
        values: &["profile", "bridge"],
        switches: &["sandbox", "dry-run", "yes"],
        run: cmd_apply::apply,
    },
    Command {
        name: "ranked-safe",
        usage: "ranked-safe [--dry-run] [--yes]",
        summary: "restore the stock ConVars block and SceneSystem values, remove the HUD addon",
        values: &[],
        switches: &["dry-run", "yes"],
        run: cmd_apply::ranked_safe,
    },
    Command {
        name: "practice",
        usage: "practice on|off [--shadows] [--fog] [--batching] --profile <file>",
        summary: "SceneSystem shortcuts for bots, sandbox and unranked (matchmaking may refuse to queue)",
        values: &["profile"],
        switches: &["shadows", "fog", "batching"],
        run: cmd_profile::practice,
    },
    Command {
        name: "restore",
        usage: "restore --original|--latest|--list [--kind gameinfo|video]",
        summary: "restore or list backups",
        values: &["kind"],
        switches: &["original", "latest", "list"],
        run: cmd_apply::restore,
    },
    Command {
        name: "push",
        usage: "push name=value ... [--bridge execfile|netcon[:port]|clipboard] [--wait <secs>]",
        summary: "send console commands now; --wait reads the game's reply from its console log",
        values: &["bridge", "wait"],
        switches: &[],
        run: cmd_apply::push,
    },
    Command {
        name: "watch",
        usage: "watch",
        summary: "print game updates and file overwrites until Ctrl-C",
        values: &[],
        switches: &[],
        run: cmd_game::watch,
    },
    Command {
        name: "launch",
        usage: "launch [--profile <file>] [--console] [-- args...]",
        summary: "start Deadlock through Steam with the args plus +exec deadtune_boot -condebug",
        values: &["profile"],
        switches: &["console"],
        run: cmd_game::launch,
    },
    Command {
        name: "launch-options check",
        usage: "launch-options check \"<args>\" | launch-options check -- <args...>",
        summary: "say which launch options this game build knows, ignores or contradicts",
        values: &[],
        switches: &[],
        run: cmd_game::launch_options_check,
    },
    Command {
        name: "kill",
        usage: "kill",
        summary: "stop Deadlock",
        values: &[],
        switches: &[],
        run: cmd_game::kill,
    },
    Command {
        name: "bench import",
        usage: "bench import <csv> --profile <name> --label <label>",
        summary: "import a PresentMon/MangoHud capture",
        values: &["profile", "label"],
        switches: &[],
        run: cmd_bench::import,
    },
    Command {
        name: "bench list",
        usage: "bench list --profile <name>",
        summary: "list bench runs for a profile",
        values: &["profile"],
        switches: &[],
        run: cmd_bench::list,
    },
    Command {
        name: "bench compare",
        usage: "bench compare <profile> <labelA> <labelB>",
        summary: "compare two runs",
        values: &[],
        switches: &[],
        run: cmd_bench::compare,
    },
    Command {
        name: "hud apply",
        usage: "hud apply --layout <hud.toml> [--diff] [--yes]",
        summary: "build and install the HUD addon (apply --profile also reconciles it to the profile's [hud])",
        values: &["layout"],
        switches: &["diff", "yes"],
        run: cmd_hud::apply,
    },
    Command {
        name: "hud remove",
        usage: "hud remove [--yes]",
        summary: "remove the DeadTune HUD addon",
        values: &[],
        switches: &["yes"],
        run: cmd_hud::remove,
    },
    Command {
        name: "hud status",
        usage: "hud status [--layout <hud.toml>]",
        summary: "addon state, search path, conflicts",
        values: &["layout"],
        switches: &[],
        run: cmd_hud::status,
    },
    Command {
        name: "addons list",
        usage: "addons list [--profile <file>]",
        summary: "performance addons: authors, download and install state",
        values: &["profile"],
        switches: &[],
        run: cmd_addons::list,
    },
    Command {
        name: "addons enable",
        usage: "addons enable <id> --profile <file> [--keep <group,...>] [--hide <group,...>]",
        summary: "turn an addon on in a profile (apply --profile installs it)",
        values: &["profile", "keep", "hide"],
        switches: &[],
        run: cmd_addons::enable,
    },
    Command {
        name: "addons disable",
        usage: "addons disable <id> --profile <file>",
        summary: "turn an addon off in a profile (apply --profile removes it)",
        values: &["profile"],
        switches: &[],
        run: cmd_addons::disable,
    },
    Command {
        name: "addons import",
        usage: "addons import <file.vpk or folder>",
        summary: "take a downloaded upstream addon file into DeadTune's cache",
        values: &[],
        switches: &[],
        run: cmd_addons::import,
    },
    Command {
        name: "addons fetch",
        usage: "addons fetch <id>|all",
        summary: "download upstream addon files from GitHub (fetch feature)",
        values: &[],
        switches: &[],
        run: cmd_addons::fetch,
    },
    Command {
        name: "addons verify",
        usage: "addons verify",
        summary: "read every installed DeadTune pak back and check it (lengths, CRCs, compiled resources)",
        values: &[],
        switches: &[],
        run: cmd_addons::verify,
    },
    Command {
        name: "addons build",
        usage: "addons build --profile <file>",
        summary: "build the texture downscaler pak from the game files",
        values: &["profile"],
        switches: &[],
        run: cmd_addons::build,
    },
    Command {
        name: "snapshot take",
        usage: "snapshot take [--categories hud,settings,menu,panorama,deadtune,config] [--no-decode] [--size-cap <MB>|none]",
        summary: "copy the game's interface and settings files into the data folder, decoded, with a manifest",
        values: &["categories", "size-cap"],
        switches: &["no-decode"],
        run: cmd_snapshot::take,
    },
    Command {
        name: "snapshot list",
        usage: "snapshot list",
        summary: "list game file snapshots, newest first",
        values: &[],
        switches: &[],
        run: cmd_snapshot::list,
    },
    Command {
        name: "snapshot diff",
        usage: "snapshot diff <old> <new> [--full]",
        summary: "compare two snapshots (names, build ids, latest, previous); the report goes into the newer one",
        values: &[],
        switches: &["full"],
        run: cmd_snapshot::diff,
    },
    Command {
        name: "self-update",
        usage: "self-update [--check] [--channel stable|testing] [--exe <deadtune.exe>] [--yes]",
        summary: "check for, verify and install a newer DeadTune (fetch feature)",
        values: &["channel", "exe"],
        switches: &["check", "yes"],
        run: cmd_update::self_update,
    },
];

const GLOBAL_HELP: &str = "Global options:
  --game-dir <path>   Deadlock folder (.../steamapps/common/Deadlock); else $DEADTUNE_GAME_DIR, else Steam lookup
  --data-dir <path>   DeadTune data (backups, HUD record, bench history); else the per-user data dir
  -h, --help          help for any command";

pub fn help() -> String {
    let mut out = String::from(
        "deadtune-cli: Deadlock config editor\n\nUsage: deadtune-cli [global options] <command> [args]\n\nCommands:\n",
    );
    for c in COMMANDS {
        out.push_str(&format!("  {:<26} {}\n", c.name, c.summary));
    }
    out.push('\n');
    out.push_str(GLOBAL_HELP);
    out
}

pub fn command_help(c: &Command) -> String {
    let bridge = if c.values.contains(&"bridge") {
        format!("\nBridges: {BRIDGE_HELP} (clipboard only for push)")
    } else {
        String::new()
    };
    format!(
        "{}\n\nUsage: deadtune-cli {}{bridge}\n\n{GLOBAL_HELP}",
        c.summary, c.usage
    )
}
