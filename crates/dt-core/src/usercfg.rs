//! The game's saved player settings, `cfg/user_convars_*.vcfg`: a KeyValues file,
//! `"config" { "convars" { "name" "value" ... } }`, written by the game whenever an archived
//! ConVar changes (and on `host_writeconfig`). Read only; DeadTune never writes it.

use std::collections::BTreeMap;
use std::path::Path;

pub const FILE_PREFIX: &str = "user_convars";
pub const FILE_SUFFIX: &str = ".vcfg";

/// The `convars` block as name to value. Anything outside `config/convars` is ignored, as
/// are lines that are not a quoted pair.
pub fn parse_convars(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut depth = 0usize;
    let mut in_convars = false;
    let mut last_key: Option<String> = None;
    for token in tokens(text) {
        match token {
            Token::Open => {
                depth += 1;
                if depth == 2 && last_key.as_deref() == Some("convars") {
                    in_convars = true;
                }
                last_key = None;
            }
            Token::Close => {
                if depth == 2 {
                    in_convars = false;
                }
                depth = depth.saturating_sub(1);
                last_key = None;
            }
            Token::Text(s) => match last_key.take() {
                Some(key) => {
                    if in_convars && depth == 2 {
                        out.insert(key, s);
                    }
                }
                None => last_key = Some(s),
            },
        }
    }
    out
}

enum Token {
    Open,
    Close,
    Text(String),
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => out.push(Token::Open),
            '}' => out.push(Token::Close),
            '"' => {
                let mut s = String::new();
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    s.push(c);
                }
                out.push(Token::Text(s));
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Every `user_convars_*.vcfg` in `cfg_dir` merged, oldest file first so the most recently
/// written slot wins. A missing folder is an empty map.
pub fn read_convars(cfg_dir: &Path) -> std::io::Result<BTreeMap<String, String>> {
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> =
        match std::fs::read_dir(cfg_dir) {
            Ok(dir) => dir
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.starts_with(FILE_PREFIX) && n.ends_with(FILE_SUFFIX))
                })
                .filter_map(|e| {
                    let modified = e.metadata().ok()?.modified().ok()?;
                    Some((modified, e.path()))
                })
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(e),
        };
    files.sort();
    let mut out = BTreeMap::new();
    for (_, path) in files {
        out.extend(parse_convars(&std::fs::read_to_string(path)?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\"config\"\r\n{\r\n\t\"convars\"\r\n\t{\r\n\t\t\"citadel_camera_hero_fov\"\t\t\"75\"\r\n\t\t\"name\"\t\t\"Frog\"\r\n\t\t\"m_yaw\"\t\t\"0.022\"\r\n\t}\r\n\t\"binds\"\r\n\t{\r\n\t\t\"f8\"\t\t\"exec dt\"\r\n\t}\r\n}\r\n";

    #[test]
    fn reads_only_the_convars_block() {
        let map = parse_convars(SAMPLE);
        assert_eq!(map.len(), 3, "{map:?}");
        assert_eq!(map["citadel_camera_hero_fov"], "75");
        assert_eq!(map["name"], "Frog");
        assert_eq!(map["m_yaw"], "0.022");
        assert!(parse_convars("").is_empty());
        assert!(parse_convars("\"config\" { \"other\" { \"a\" \"b\" } }").is_empty());
        let nested = "\"config\" { \"convars\" { \"x\" { \"deep\" \"1\" } \"y\" \"2\" } }";
        assert_eq!(
            parse_convars(nested).get("y").map(String::as_str),
            Some("2")
        );
        assert!(!parse_convars(nested).contains_key("deep"));
        assert_eq!(
            parse_convars("// note\n\"config\" { \"convars\" { \"a\" \"1\" // trailing\n } }")["a"],
            "1"
        );
    }

    #[test]
    fn newest_slot_file_wins_and_missing_folder_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_convars(&dir.path().join("nope")).unwrap().is_empty());
        let cfg = dir.path().join("cfg");
        std::fs::create_dir(&cfg).unwrap();
        let old = cfg.join("user_convars_0_slot0.vcfg");
        let new = cfg.join("user_convars_1_slot0.vcfg");
        std::fs::write(
            &old,
            "\"config\" { \"convars\" { \"a\" \"old\" \"b\" \"1\" } }",
        )
        .unwrap();
        std::fs::write(&new, "\"config\" { \"convars\" { \"a\" \"new\" } }").unwrap();
        std::fs::write(
            cfg.join("video.txt"),
            "\"config\" { \"convars\" { \"a\" \"x\" } }",
        )
        .unwrap();
        let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(earlier)
            .unwrap();
        let map = read_convars(&cfg).unwrap();
        assert_eq!(map["a"], "new");
        assert_eq!(map["b"], "1");
    }
}
