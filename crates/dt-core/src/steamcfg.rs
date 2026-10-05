//! Read-only look at Steam's per-user config: the launch options set for Deadlock in
//! `userdata/<id>/config/localconfig.vdf`.

use std::path::Path;

pub const APPID: &str = "1422450";

/// Quoted strings and braces of a KeyValues (VDF) text; comments skipped.
fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => out.push(Token::Open),
            '}' => out.push(Token::Close),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(next) = chars.next() {
                                s.push(match next {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        '"' => break,
                        other => s.push(other),
                    }
                }
                out.push(Token::Str(s));
            }
            _ => {}
        }
    }
    out
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

/// `LaunchOptions` under `apps/<appid>`, if the user set any.
pub fn launch_options(vdf: &str, appid: &str) -> Option<String> {
    let mut path: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for token in tokens(vdf) {
        match token {
            Token::Open => path.push(pending.take().unwrap_or_default()),
            Token::Close => {
                path.pop();
                pending = None;
            }
            Token::Str(s) => match pending.take() {
                None => pending = Some(s),
                Some(key) => {
                    let in_app = path.len() >= 2
                        && path[path.len() - 2].eq_ignore_ascii_case("apps")
                        && path[path.len() - 1] == appid;
                    if in_app && key.eq_ignore_ascii_case("LaunchOptions") {
                        return Some(s);
                    }
                }
            },
        }
    }
    None
}

/// Deadlock's launch options for every Steam user on this PC that has some.
pub fn read_launch_options(steam_root: &Path) -> Vec<String> {
    let Ok(users) = std::fs::read_dir(steam_root.join("userdata")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = users
        .filter_map(|u| u.ok())
        .filter_map(|u| std::fs::read_to_string(u.path().join("config/localconfig.vdf")).ok())
        .filter_map(|text| launch_options(&text, APPID))
        .filter(|o| !o.trim().is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `+name value` pairs that set a console variable or run a command at startup, except
/// DeadTune's own boot config.
pub fn console_overrides(options: &str) -> Vec<String> {
    let words: Vec<&str> = options.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if let Some(name) = words[i].strip_prefix('+') {
            let value = words.get(i + 1).filter(|w| !w.starts_with(['+', '-']));
            let ours = name == "exec" && value.is_some_and(|v| v.starts_with("deadtune"));
            if !ours {
                out.push(match value {
                    Some(v) => format!("+{name} {v}"),
                    None => format!("+{name}"),
                });
            }
            i += 1 + usize::from(value.is_some());
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCALCONFIG: &str = r#""UserLocalConfigStore"
{
	"Software"
	{
		"Valve"
		{
			"Steam"
			{
				"apps"
				{
					"570"
					{
						"LaunchOptions"		"-novid"
					}
					"1422450"
					{
						"LastPlayed"		"1791000000"
						// a comment
						"LaunchOptions"		"-dx11 +fps_max 144 +exec deadtune_boot -condebug"
					}
				}
			}
		}
	}
}
"#;

    #[test]
    fn reads_deadlocks_launch_options_only() {
        assert_eq!(
            launch_options(LOCALCONFIG, APPID).as_deref(),
            Some("-dx11 +fps_max 144 +exec deadtune_boot -condebug")
        );
        assert_eq!(
            launch_options(LOCALCONFIG, "570").as_deref(),
            Some("-novid")
        );
        assert_eq!(launch_options(LOCALCONFIG, "730"), None);
        assert_eq!(launch_options("\"a\" { \"b\" \"c\\\"d\" }", APPID), None);
    }

    #[test]
    fn console_overrides_skip_deadtunes_own_boot_cfg() {
        assert_eq!(
            console_overrides("-dx11 +fps_max 144 +exec deadtune_boot -condebug"),
            vec!["+fps_max 144"]
        );
        assert_eq!(
            console_overrides("-high +exec autoexec +r_shadows"),
            vec!["+exec autoexec", "+r_shadows"]
        );
        assert!(console_overrides("-novid -dx11").is_empty());
    }

    #[test]
    fn reads_every_user_with_options() {
        let dir = tempfile::tempdir().unwrap();
        for (user, text) in [("111", LOCALCONFIG), ("222", "\"x\" {}")] {
            let cfg = dir.path().join("userdata").join(user).join("config");
            std::fs::create_dir_all(&cfg).unwrap();
            std::fs::write(cfg.join("localconfig.vdf"), text).unwrap();
        }
        assert_eq!(
            read_launch_options(dir.path()),
            vec!["-dx11 +fps_max 144 +exec deadtune_boot -condebug"]
        );
        assert!(read_launch_options(&dir.path().join("missing")).is_empty());
    }
}
