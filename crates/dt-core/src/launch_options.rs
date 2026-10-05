//! Launch options: a typed model with friendly toggles plus free-form extras, and a checker
//! that tells which arguments this game build knows.
//!
//! The known list is the game's own (`research/configs/OptimizationLock/launch_options.txt`,
//! one option per line, no descriptions). Many Source 1 tips found in configs, such as
//! `-nosplash`, `-noaaf` and `-noshadows`, are not in it, which is why the checker exists.

use std::sync::OnceLock;

use crate::catalog::Catalog;
use crate::launch::{BOOT_CFG, CONDEBUG};

const KNOWN_LIST: &str =
    include_str!("../../../research/configs/OptimizationLock/launch_options.txt");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Renderer {
    #[default]
    Default,
    Vulkan,
    Dx11,
}

impl Renderer {
    pub const ALL: [Renderer; 3] = [Renderer::Default, Renderer::Vulkan, Renderer::Dx11];

    pub fn flag(self) -> Option<&'static str> {
        match self {
            Renderer::Default => None,
            Renderer::Vulkan => Some("-vulkan"),
            Renderer::Dx11 => Some("-dx11"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Renderer::Default => "Default",
            Renderer::Vulkan => "Vulkan",
            Renderer::Dx11 => "DirectX 11",
        }
    }
}

/// What the user wants on the command line, before DeadTune adds its boot options.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(from = "Stored")]
pub struct LaunchOptions {
    pub renderer: Renderer,
    /// `-novid`.
    pub skip_intro: bool,
    /// Everything else, one command-line token per entry, in the user's order.
    pub extra: Vec<String>,
}

impl Default for LaunchOptions {
    fn default() -> LaunchOptions {
        LaunchOptions {
            renderer: Renderer::Default,
            skip_intro: true,
            extra: Vec::new(),
        }
    }
}

impl LaunchOptions {
    /// Lifts the first renderer flag and the first `-novid` into toggles; every other token,
    /// repeats included, stays in `extra` in order, so `args()` holds the same tokens.
    pub fn from_args(args: &[String]) -> LaunchOptions {
        let mut opts = LaunchOptions {
            renderer: Renderer::Default,
            skip_intro: false,
            extra: Vec::new(),
        };
        for arg in args {
            let renderer = Renderer::ALL
                .into_iter()
                .find(|r| r.flag().is_some_and(|f| f.eq_ignore_ascii_case(arg)));
            match renderer {
                Some(r) if opts.renderer == Renderer::Default => opts.renderer = r,
                _ if !opts.skip_intro && arg.eq_ignore_ascii_case("-novid") => {
                    opts.skip_intro = true
                }
                _ => opts.extra.push(arg.clone()),
            }
        }
        opts
    }

    pub fn args(&self) -> Vec<String> {
        self.renderer
            .flag()
            .into_iter()
            .chain(self.skip_intro.then_some("-novid"))
            .map(str::to_string)
            .chain(self.extra.iter().cloned())
            .collect()
    }
}

/// The settings file shape: the model's fields, or the pre-model `args = [...]` list.
#[derive(serde::Deserialize)]
#[serde(default)]
struct Stored {
    renderer: Renderer,
    skip_intro: bool,
    extra: Vec<String>,
    args: Option<Vec<String>>,
}

impl Default for Stored {
    fn default() -> Stored {
        let d = LaunchOptions::default();
        Stored {
            renderer: d.renderer,
            skip_intro: d.skip_intro,
            extra: d.extra,
            args: None,
        }
    }
}

impl From<Stored> for LaunchOptions {
    fn from(s: Stored) -> LaunchOptions {
        match s.args {
            Some(args) => LaunchOptions::from_args(&args),
            None => LaunchOptions {
                renderer: s.renderer,
                skip_intro: s.skip_intro,
                extra: s.extra,
            },
        }
    }
}

/// Splits a typed command line on whitespace; double quotes group a token with spaces.
pub fn split_command_line(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut token));
                    started = false;
                }
            }
            c => {
                token.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(token);
    }
    out
}

/// One option as the game reads it: a `-flag` or `+command` and the bare tokens after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arg {
    pub flag: String,
    pub values: Vec<String>,
}

impl Arg {
    pub fn text(&self) -> String {
        std::iter::once(&self.flag)
            .chain(&self.values)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// DeadTune adds this itself for live changes.
    Managed,
    /// In this game build's list of launch options.
    Known,
    /// A `+command` that runs a console command or sets a setting at start.
    Console,
    /// Not in this game build's list, so the game most likely ignores it.
    Unknown,
    /// Asks for something another option on the line contradicts.
    Conflicts { with: String },
}

impl Verdict {
    pub fn summary(&self) -> String {
        match self {
            Verdict::Managed => "added by DeadTune".into(),
            Verdict::Known => "known".into(),
            Verdict::Console => "console command".into(),
            Verdict::Unknown => "not in this game build: probably does nothing".into(),
            Verdict::Conflicts { with } => format!("conflicts with {with}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checked {
    pub arg: Arg,
    pub verdict: Verdict,
}

/// Each inner slice is one mode with its aliases; two modes of one group contradict each other.
const EXCLUSIVE: &[&[&[&str]]] = &[
    &[&["-vulkan"], &["-dx11"]],
    &[&["-fullscreen", "-full"], &["-windowed", "-sw"]],
];

fn group(args: &[String]) -> Vec<Arg> {
    let mut out: Vec<Arg> = Vec::new();
    for token in args {
        match out.last_mut() {
            Some(last) if !token.starts_with(['-', '+']) => last.values.push(token.clone()),
            _ => out.push(Arg {
                flag: token.clone(),
                values: Vec::new(),
            }),
        }
    }
    out
}

pub fn check(args: &[String]) -> Vec<Checked> {
    let grouped = group(args);
    let flags: Vec<String> = grouped
        .iter()
        .map(|a| a.flag.to_ascii_lowercase())
        .collect();
    grouped
        .into_iter()
        .zip(&flags)
        .map(|(arg, flag)| {
            let verdict = verdict(&arg, flag, &flags);
            Checked { arg, verdict }
        })
        .collect()
}

fn verdict(arg: &Arg, flag: &str, all: &[String]) -> Verdict {
    if is_managed(arg, flag) {
        return Verdict::Managed;
    }
    if let Some(with) = conflict(flag, all) {
        return Verdict::Conflicts { with };
    }
    if is_known(flag) {
        return Verdict::Known;
    }
    match flag.strip_prefix('+') {
        Some("exec") => Verdict::Console,
        Some(name) if Catalog::embedded().get(name).is_some() => Verdict::Console,
        _ => Verdict::Unknown,
    }
}

fn is_managed(arg: &Arg, flag: &str) -> bool {
    flag == CONDEBUG
        || (flag == "+exec"
            && arg
                .values
                .first()
                .is_some_and(|v| v.trim_end_matches(".cfg") == BOOT_CFG))
}

fn conflict(flag: &str, all: &[String]) -> Option<String> {
    for group in EXCLUSIVE {
        let Some(mine) = group.iter().position(|mode| mode.contains(&flag)) else {
            continue;
        };
        let other = all.iter().find(|f| {
            group
                .iter()
                .enumerate()
                .any(|(i, mode)| i != mine && mode.contains(&f.as_str()))
        });
        if let Some(other) = other {
            return Some(other.clone());
        }
    }
    None
}

/// Case-insensitive, like the engine's own command-line lookup.
fn is_known(flag: &str) -> bool {
    known_list()
        .binary_search(&flag.to_ascii_lowercase())
        .is_ok()
}

fn known_list() -> &'static [String] {
    static LIST: OnceLock<Vec<String>> = OnceLock::new();
    LIST.get_or_init(|| {
        let mut list: Vec<String> = KNOWN_LIST
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        list.sort();
        list.dedup();
        list
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    fn verdicts(line: &str) -> Vec<(String, Verdict)> {
        check(&split_command_line(line))
            .into_iter()
            .map(|c| (c.arg.text(), c.verdict))
            .collect()
    }

    #[test]
    fn the_known_list_is_the_games_full_list() {
        assert_eq!(KNOWN_LIST.lines().count(), 1536);
        assert_eq!(
            known_list().len(),
            1533,
            "three options differ only by case"
        );
        for flag in [
            "-vulkan",
            "-dx11",
            "-novid",
            "-high",
            "-threads",
            "+map",
            "-steamBeta",
        ] {
            assert!(is_known(flag), "{flag} is in the game's list");
        }
        for flag in [
            "-nosplash",
            "-noaaf",
            "-noshadows",
            "-disableframecap",
            "-dx12",
        ] {
            assert!(!is_known(flag), "{flag} is not in the game's list");
        }
    }

    #[test]
    fn sidelock_recommendation_is_classified_per_option() {
        let line = "-vulkan -novid -nosplash -high -noborder -novsync -dx11 -threads 10 -noaaf \
                    -noshadows -nod3d9ex -disableframecap";
        let conflict = |with: &str| Verdict::Conflicts { with: with.into() };
        assert_eq!(
            verdicts(line),
            [
                ("-vulkan".into(), conflict("-dx11")),
                ("-novid".into(), Verdict::Known),
                ("-nosplash".into(), Verdict::Unknown),
                ("-high".into(), Verdict::Known),
                ("-noborder".into(), Verdict::Known),
                ("-novsync".into(), Verdict::Known),
                ("-dx11".into(), conflict("-vulkan")),
                ("-threads 10".into(), Verdict::Known),
                ("-noaaf".into(), Verdict::Unknown),
                ("-noshadows".into(), Verdict::Unknown),
                ("-nod3d9ex".into(), Verdict::Known),
                ("-disableframecap".into(), Verdict::Unknown),
            ]
        );
    }

    #[test]
    fn boot_options_are_managed_and_plus_commands_are_console() {
        assert_eq!(
            verdicts("+exec deadtune_boot.cfg -condebug +exec autoexec +fps_max 0 +bogus_cmd 1"),
            [
                ("+exec deadtune_boot.cfg".into(), Verdict::Managed),
                ("-condebug".into(), Verdict::Managed),
                ("+exec autoexec".into(), Verdict::Console),
                ("+fps_max 0".into(), Verdict::Console),
                ("+bogus_cmd 1".into(), Verdict::Unknown),
            ]
        );
    }

    #[test]
    fn window_mode_aliases_conflict_and_lookup_ignores_case() {
        assert_eq!(
            verdicts("-full -SW -NOVID"),
            [
                ("-full".into(), Verdict::Conflicts { with: "-sw".into() }),
                (
                    "-SW".into(),
                    Verdict::Conflicts {
                        with: "-full".into()
                    }
                ),
                ("-NOVID".into(), Verdict::Known),
            ]
        );
    }

    #[test]
    fn a_leading_bare_value_is_its_own_unknown_option() {
        assert_eq!(
            verdicts("10 -high"),
            [
                ("10".into(), Verdict::Unknown),
                ("-high".into(), Verdict::Known)
            ]
        );
    }

    #[test]
    fn split_honours_double_quotes() {
        assert_eq!(
            split_command_line("  -novid  +exec \"my cfg\" \"\" -x"),
            ["-novid", "+exec", "my cfg", "", "-x"]
        );
    }

    #[test]
    fn toggles_produce_their_flags_before_the_extras() {
        let opts = LaunchOptions {
            renderer: Renderer::Vulkan,
            skip_intro: true,
            extra: strs(&["-high", "+fps_max", "0"]),
        };
        assert_eq!(opts.args(), ["-vulkan", "-novid", "-high", "+fps_max", "0"]);
        assert_eq!(
            LaunchOptions {
                renderer: Renderer::Dx11,
                skip_intro: false,
                extra: vec![],
            }
            .args(),
            ["-dx11"]
        );
        assert!(
            LaunchOptions {
                skip_intro: false,
                ..LaunchOptions::default()
            }
            .args()
            .is_empty()
        );
        assert_eq!(LaunchOptions::default().args(), ["-novid"]);
    }

    #[test]
    fn from_args_lifts_toggles_and_keeps_every_token() {
        let cases: &[&[&str]] = &[
            &[],
            &["-novid"],
            &["-high", "-vulkan", "-threads", "10", "-novid"],
            &["-vulkan", "-dx11", "-novid", "-novid", "+exec", "x"],
            &["-dx11", "-vulkan"],
        ];
        for case in cases {
            let args = strs(case);
            let opts = LaunchOptions::from_args(&args);
            let mut before = args.clone();
            let mut after = opts.args();
            before.sort();
            after.sort();
            assert_eq!(before, after, "{case:?} -> {opts:?}");
        }
        let opts = LaunchOptions::from_args(&strs(&["-high", "-dx11", "-vulkan", "-novid"]));
        assert_eq!(opts.renderer, Renderer::Dx11);
        assert!(opts.skip_intro);
        assert_eq!(opts.extra, ["-high", "-vulkan"]);
        assert_eq!(
            LaunchOptions::from_args(&strs(&["-Vulkan", "-NOVID"])).args(),
            ["-vulkan", "-novid"],
            "a toggle's flag is matched without case"
        );
    }

    #[test]
    fn settings_with_the_old_args_list_migrate_into_the_model() {
        let old: LaunchOptions =
            toml::from_str(r#"args = ["-novid", "-vulkan", "-high"]"#).unwrap();
        assert_eq!(
            old,
            LaunchOptions {
                renderer: Renderer::Vulkan,
                skip_intro: true,
                extra: strs(&["-high"]),
            }
        );
        let empty: LaunchOptions = toml::from_str("args = []").unwrap();
        assert!(empty.args().is_empty(), "an empty old list stays empty");

        let text = toml::to_string(&old).unwrap();
        assert!(!text.contains("args"), "{text}");
        assert_eq!(toml::from_str::<LaunchOptions>(&text).unwrap(), old);
        assert_eq!(
            toml::from_str::<LaunchOptions>("").unwrap(),
            LaunchOptions::default()
        );
    }
}
