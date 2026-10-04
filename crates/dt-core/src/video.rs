//! `video.txt` reader/writer. Line-based like `gi`: keeps the header and unknown lines.

use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VideoError {
    #[error("video.txt has no settings block")]
    NoBlock,
}

/// `"setting.x" "v"` pairs in file order.
pub fn read_settings(text: &str) -> Result<Vec<(String, String)>, VideoError> {
    let doc = Doc::parse(text)?;
    Ok(doc
        .settings()
        .map(|(_, s)| (s.key.to_string(), s.value.to_string()))
        .collect())
}

/// Sets existing keys in place; missing keys are appended before the closing brace.
pub fn apply_settings(
    text: &str,
    settings: &BTreeMap<String, String>,
) -> Result<String, VideoError> {
    let doc = Doc::parse(text)?;
    let mut out = String::with_capacity(text.len());
    for (i, line) in doc.lines.iter().enumerate() {
        if i == doc.close {
            for (key, value) in settings
                .iter()
                .filter(|(k, _)| !doc.settings().any(|(_, s)| s.key == *k))
            {
                out.push_str(&doc.format(key, value));
            }
        }
        match doc
            .setting(i)
            .and_then(|s| settings.get(s.key).map(|v| (s, v)))
        {
            Some((s, value)) => {
                out.push_str(s.before_value);
                out.push_str(value);
                out.push_str(s.after_value);
            }
            None => out.push_str(line),
        }
    }
    Ok(out)
}

/// Keeps `target`'s header (device id lines etc.) and takes every setting from `source`.
pub fn replace_settings(target: &str, source: &str) -> Result<String, VideoError> {
    let doc = Doc::parse(target)?;
    let incoming = read_settings(source)?;
    let insert_at = doc.settings().next().map_or(doc.close, |(i, _)| i);
    let mut out = String::with_capacity(target.len());
    for (i, line) in doc.lines.iter().enumerate() {
        if i == insert_at {
            for (key, value) in &incoming {
                out.push_str(&doc.format(key, value));
            }
        }
        if doc.setting(i).is_none() {
            out.push_str(line);
        }
    }
    Ok(out)
}

struct Setting<'a> {
    key: &'a str,
    value: &'a str,
    indent: &'a str,
    pad: &'a str,
    /// The raw line up to and including the value's opening quote.
    before_value: &'a str,
    /// The raw line from the value's closing quote on, EOL included.
    after_value: &'a str,
}

struct Doc<'a> {
    lines: Vec<&'a str>,
    parsed: Vec<Option<Setting<'a>>>,
    /// Some shipped presets (OptiLock) omit the `"video.cfg" {` header, so the
    /// closing brace is the only reliable anchor for the settings block.
    close: usize,
    eol: &'static str,
}

impl<'a> Doc<'a> {
    fn parse(text: &'a str) -> Result<Self, VideoError> {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let close = lines
            .iter()
            .rposition(|l| l.trim() == "}")
            .ok_or(VideoError::NoBlock)?;
        let parsed = lines
            .iter()
            .enumerate()
            .map(|(i, l)| (i < close).then(|| parse_setting(l)).flatten())
            .collect();
        let eol = if lines.first().is_some_and(|l| l.ends_with("\r\n")) {
            "\r\n"
        } else {
            "\n"
        };
        Ok(Doc {
            lines,
            parsed,
            close,
            eol,
        })
    }

    fn setting(&self, i: usize) -> Option<&Setting<'a>> {
        self.parsed[i].as_ref()
    }

    fn settings(&self) -> impl Iterator<Item = (usize, &Setting<'a>)> {
        self.parsed
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|s| (i, s)))
    }

    /// A new line in the style of the file's last setting line.
    fn format(&self, key: &str, value: &str) -> String {
        let (indent, pad) = self
            .settings()
            .last()
            .map_or(("\t", "\t\t"), |(_, s)| (s.indent, s.pad));
        format!("{indent}\"{key}\"{pad}\"{value}\"{}", self.eol)
    }
}

fn parse_setting(line: &str) -> Option<Setting<'_>> {
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let indent = &line[..indent_len];
    let rest = line[indent_len..].strip_prefix('"')?;
    let key_end = rest.find('"')?;
    let key = &rest[..key_end];
    if !key.starts_with("setting.") {
        return None;
    }
    let after_key = &rest[key_end + 1..];
    let pad_len = after_key.len() - after_key.trim_start_matches([' ', '\t']).len();
    let value_and_rest = after_key[pad_len..].strip_prefix('"')?;
    let value_end = value_and_rest.find('"')?;
    let value_start = line.len() - value_and_rest.len();
    Some(Setting {
        key,
        value: &value_and_rest[..value_end],
        indent,
        pad: &after_key[..pad_len],
        before_value: &line[..value_start],
        after_value: &line[value_start + value_end..],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn research(rel: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../research")
            .join(rel)
    }

    fn fixture(rel: &str) -> String {
        std::fs::read_to_string(research(rel)).unwrap()
    }

    fn all_video_files() -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.file_name().is_some_and(|n| n == "video.txt") {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(&research("configs"), &mut out);
        assert!(
            out.len() >= 13,
            "expected every research video.txt, found {}",
            out.len()
        );
        out
    }

    /// Independent oracle: every `"setting.x" "v"` line in file order.
    fn naive_pairs(text: &str) -> Vec<(String, String)> {
        text.lines()
            .filter_map(|line| {
                let mut quoted = line.split('"').skip(1).step_by(2);
                let key = quoted.next()?;
                let value = quoted.next()?;
                key.starts_with("setting.")
                    .then(|| (key.to_string(), value.to_string()))
            })
            .collect()
    }

    fn to_crlf(text: &str) -> String {
        text.replace("\r\n", "\n").replace('\n', "\r\n")
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    const TEST_CFG: &str = "configs/OptimizationLock/test_cfg/video.txt";
    const POTATO: &str = "configs/OptiLock/OptiLock Potato Config/video.txt";
    const RECOMMENDED: &str = "configs/OptiLock/OptiLock FPS Config (Recommended)/video.txt";
    const PIGGY: &str =
        "configs/OptimizationLock/piggy's config (comparatively outdated)/video.txt";

    #[test]
    fn read_settings_returns_every_pair_in_file_order() {
        for path in all_video_files() {
            let text = std::fs::read_to_string(&path).unwrap();
            let pairs = read_settings(&text).unwrap();
            assert!(!pairs.is_empty(), "{} has no settings", path.display());
            assert_eq!(pairs, naive_pairs(&text), "{}", path.display());
        }
    }

    #[test]
    fn read_settings_keeps_duplicates_and_skips_header_keys() {
        let pairs = read_settings(&fixture(POTATO)).unwrap();
        let dupes = pairs
            .iter()
            .filter(|(k, _)| k == "setting.r_particle_max_detail_level")
            .count();
        assert_eq!(
            dupes, 2,
            "OptiLock Potato lists r_particle_max_detail_level twice"
        );
        let header = read_settings(&fixture(TEST_CFG)).unwrap();
        assert!(header.iter().all(|(k, _)| k.starts_with("setting.")));
        assert_eq!(header[0], ("setting.cpu_level".into(), "0".into()));
    }

    #[test]
    fn read_settings_without_closing_brace_is_no_block() {
        assert_eq!(
            read_settings("\"setting.a\"\t\t\"1\"\n"),
            Err(VideoError::NoBlock)
        );
    }

    #[test]
    fn apply_empty_map_is_byte_identical() {
        for path in all_video_files() {
            let text = std::fs::read_to_string(&path).unwrap();
            assert_eq!(
                apply_settings(&text, &BTreeMap::new()).unwrap(),
                text,
                "{}",
                path.display()
            );
            let crlf = to_crlf(&text);
            assert_eq!(
                apply_settings(&crlf, &BTreeMap::new()).unwrap(),
                crlf,
                "CRLF {}",
                path.display()
            );
        }
    }

    #[test]
    fn apply_existing_key_changes_only_that_value() {
        let text = fixture(TEST_CFG);
        let out = apply_settings(&text, &map(&[("setting.cpu_level", "12")])).unwrap();
        let changed: Vec<(&str, &str)> = text
            .split_inclusive('\n')
            .zip(out.split_inclusive('\n'))
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(
            changed,
            [(
                "\t\"setting.cpu_level\"\t\t\"0\"\r\n",
                "\t\"setting.cpu_level\"\t\t\"12\"\r\n"
            )]
        );
        assert_eq!(out.len(), text.len() + 1);
    }

    #[test]
    fn apply_existing_key_keeps_space_padding_and_trailing_comment() {
        let text = fixture(PIGGY);
        let out = apply_settings(&text, &map(&[("setting.fps_max", "60")])).unwrap();
        assert!(out.contains(
            "\"setting.fps_max\" \"60\"   \t\t\t\t// Change to cap fps (Leave at 0 if using RTSS)\n"
        ));
        assert_eq!(out.lines().count(), text.lines().count());
    }

    #[test]
    fn apply_sets_every_duplicate() {
        let out = apply_settings(
            &fixture(POTATO),
            &map(&[("setting.r_particle_max_detail_level", "1")]),
        )
        .unwrap();
        let values: Vec<String> = read_settings(&out)
            .unwrap()
            .into_iter()
            .filter(|(k, _)| k == "setting.r_particle_max_detail_level")
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, ["1", "1"]);
    }

    #[test]
    fn apply_appends_new_keys_before_closing_brace_with_file_style() {
        let text = fixture(TEST_CFG);
        let out = apply_settings(
            &text,
            &map(&[("setting.new_a", "1"), ("setting.new_b", "2")]),
        )
        .unwrap();
        let tail = "\t\"setting.new_a\"\t\t\"1\"\r\n\t\"setting.new_b\"\t\t\"2\"\r\n}\r\n";
        assert!(
            out.ends_with(tail),
            "tab indent, tab padding and CRLF: {:?}",
            &out[out.len() - 80..]
        );
        assert!(out.starts_with(&text[..text.len() - "}\r\n".len()]));
    }

    #[test]
    fn apply_appends_into_file_without_trailing_newline() {
        let text = fixture(RECOMMENDED);
        assert!(text.ends_with("\n}"));
        let out = apply_settings(&text, &map(&[("setting.new_a", "1")])).unwrap();
        assert!(out.ends_with("\"\n\t\"setting.new_a\"\t\t\"1\"\n}"));
    }

    #[test]
    fn apply_appends_with_unindented_space_style() {
        let out = apply_settings(&fixture(PIGGY), &map(&[("setting.new_a", "1")])).unwrap();
        assert!(out.ends_with("\n\n\n\"setting.new_a\" \"1\"\n}\n"));
    }

    #[test]
    fn replace_keeps_target_header_and_takes_source_settings() {
        let target = fixture(TEST_CFG);
        let header = "\"video.cfg\"\r\n{\r\n\t\"Version\"\t\t\"20\"\r\n\t\"VendorID\"\t\t\"\"\r\n\t\"DeviceID\"\t\t\"\"\r\n";
        for source in [fixture(POTATO), fixture(PIGGY)] {
            let out = replace_settings(&target, &source).unwrap();
            assert_eq!(
                read_settings(&out).unwrap(),
                read_settings(&source).unwrap()
            );
            assert!(out.starts_with(header));
            assert!(out.ends_with("\r\n}\r\n"));
            assert!(
                !out.replace("\r\n", "").contains('\n'),
                "target EOL kept everywhere"
            );
            assert!(out.contains("\r\n\t\"setting.r_texture_stream_mip_bias\"\t\t\"4\"\r\n"));
        }
    }

    #[test]
    fn replace_into_piggy_keeps_its_comment_header() {
        let out = replace_settings(&fixture(PIGGY), &fixture(POTATO)).unwrap();
        assert_eq!(
            read_settings(&out).unwrap(),
            read_settings(&fixture(POTATO)).unwrap()
        );
        for header in [
            "\"Version\"\t\t\"20\"\t",
            "\"VendorID\"\t\t\" \"\t//replace with the value found in your config",
        ] {
            assert!(out.contains(header), "kept {header:?}");
        }
    }

    #[test]
    fn replace_requires_blocks() {
        assert_eq!(
            replace_settings("no block", &fixture(POTATO)),
            Err(VideoError::NoBlock)
        );
        assert_eq!(
            replace_settings(&fixture(TEST_CFG), "no block"),
            Err(VideoError::NoBlock)
        );
    }
}
