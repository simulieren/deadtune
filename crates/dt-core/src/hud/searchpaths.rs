//! Addon VPKs only mount when `gameinfo.gi` SearchPaths lists `Game citadel/addons`.
//! Pure text edits; the apply pipeline decides when to write. Touches nothing but
//! the `SearchPaths` block, preserving EOL style and indentation. The line goes
//! right before the first plain `Game` entry, so language paths stay mounted first.

pub const ADDONS_LINE_VALUE: &str = "citadel/addons";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SearchPathsError {
    #[error("no SearchPaths block in gameinfo.gi")]
    Missing,
    #[error("unbalanced braces in SearchPaths")]
    Unbalanced,
}

/// One entry line directly inside SearchPaths.
struct Entry<'a> {
    /// Byte offset of the line start.
    start: usize,
    indent: &'a str,
    key: &'a str,
    /// Whitespace between key and value.
    gap: &'a str,
    value: &'a str,
}

struct Block<'a> {
    entries: Vec<Entry<'a>>,
    /// Byte offset of the line holding the closing brace.
    close_line_start: usize,
    close_indent: &'a str,
}

/// True if an uncommented `Game citadel/addons` (any whitespace, optional quotes) exists.
pub fn has_addons(gameinfo: &str) -> Result<bool, SearchPathsError> {
    Ok(find_addons(&parse_block(gameinfo)?))
}

/// Inserts `Game citadel/addons` immediately before the first plain `Game` entry, after
/// the language, low-violence, `Mod` and `Write` entries (the layout DMM and community
/// presets use), so addons win over `Game citadel`. With no plain `Game` entry it goes
/// before the closing brace. Returns the input unchanged if already present.
pub fn ensure_addons(gameinfo: &str) -> Result<String, SearchPathsError> {
    let block = parse_block(gameinfo)?;
    if find_addons(&block) {
        return Ok(gameinfo.to_owned());
    }
    let eol = match crate::gi::detect_eol(gameinfo) {
        crate::gi::Eol::CrLf => "\r\n",
        crate::gi::Eol::Lf => "\n",
    };

    let anchor = block.entries.iter().find(|e| e.key.eq_ignore_ascii_case("Game"));
    let (at, indent, key_gap) = match (anchor, block.entries.last()) {
        (Some(e), _) => (e.start, e.indent.to_owned(), key_gap_for(e)),
        (None, Some(last)) => (block.close_line_start, last.indent.to_owned(), key_gap_for(last)),
        (None, None) => (
            block.close_line_start,
            format!("{}    ", block.close_indent),
            "\t\t\t\t".to_owned(),
        ),
    };

    let line = format!("{indent}Game{key_gap}{ADDONS_LINE_VALUE}{eol}");
    let mut out = String::with_capacity(gameinfo.len() + line.len());
    out.push_str(&gameinfo[..at]);
    out.push_str(&line);
    out.push_str(&gameinfo[at..]);
    Ok(out)
}

fn find_addons(block: &Block) -> bool {
    block
        .entries
        .iter()
        .any(|e| e.key.eq_ignore_ascii_case("Game") && e.value == ADDONS_LINE_VALUE)
}

/// Whitespace between `Game` and the value, matching the first entry's style:
/// tabs stay tabs, spaces are padded so values line up in the same column.
fn key_gap_for(first: &Entry) -> String {
    if first.gap.contains('\t') {
        return "\t\t\t\t".to_owned();
    }
    let value_col = first.key.len() + first.gap.len();
    " ".repeat(value_col.saturating_sub("Game".len()).max(1))
}

/// Line content without `//` comment (quote-aware) and without the line terminator.
fn strip_comment(line: &str) -> &str {
    let line = line.trim_end_matches(['\n', '\r']);
    let mut in_quote = false;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quote = !in_quote,
            b'/' if !in_quote && bytes.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
    }
    line
}

fn parse_entry(start: usize, code: &str) -> Option<Entry<'_>> {
    let trimmed = code.trim_start();
    let indent = &code[..code.len() - trimmed.len()];
    let trimmed = trimmed.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let key_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let key = &trimmed[..key_end];
    let rest = &trimmed[key_end..];
    let after_gap = rest.trim_start();
    let gap = &rest[..rest.len() - after_gap.len()];
    let value = match after_gap.strip_prefix('"') {
        Some(q) => q.split('"').next().unwrap_or(""),
        None => after_gap.split_whitespace().next().unwrap_or(""),
    };
    Some(Entry { start, indent, key, gap, value })
}

fn parse_block(text: &str) -> Result<Block<'_>, SearchPathsError> {
    let mut offset = 0;
    let mut lines = text.split_inclusive('\n').map(|l| {
        let start = offset;
        offset += l.len();
        (start, l)
    });

    let mut found = false;
    for (_, line) in lines.by_ref() {
        if strip_comment(line).trim() == "SearchPaths" {
            found = true;
            break;
        }
    }
    if !found {
        return Err(SearchPathsError::Missing);
    }

    let mut depth = 0usize;
    let mut opened = false;
    let mut entries = Vec::new();
    for (start, line) in lines {
        let code = strip_comment(line);
        let at_line_start = depth;
        for c in code.chars() {
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => {
                    depth = depth.checked_sub(1).ok_or(SearchPathsError::Unbalanced)?;
                }
                _ => {}
            }
        }
        if !opened {
            if code.trim().is_empty() {
                continue;
            }
            return Err(SearchPathsError::Unbalanced);
        }
        if opened && depth == 0 {
            let trimmed = code.trim_start();
            return Ok(Block {
                entries,
                close_line_start: start,
                close_indent: &code[..code.len() - trimmed.len()],
            });
        }
        if at_line_start == 1 && depth == 1 && !code.contains(['{', '}']) {
            entries.extend(parse_entry(start, code));
        }
    }
    Err(SearchPathsError::Unbalanced)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VANILLA: &str = include_str!(
        "../../../../research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi"
    );
    const PRESET: &str =
        include_str!("../../../../research/configs/OptimizationLock/test_cfg/gameinfo.gi");

    fn crlf(s: &str) -> String {
        s.replace("\r\n", "\n").replace('\n', "\r\n")
    }

    /// Bytes before and after the SearchPaths block's inner lines.
    fn outside(text: &str) -> (String, String) {
        let open = text.find("SearchPaths").unwrap();
        let brace = open + text[open..].find('{').unwrap();
        let head_end = brace + text[brace..].find('\n').unwrap() + 1;
        let mut depth = 0;
        let mut close = 0;
        for (i, c) in text[brace..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = brace + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let tail_start = text[..close].rfind('\n').map_or(0, |i| i + 1);
        (text[..head_end].to_owned(), text[tail_start..].to_owned())
    }

    fn assert_only_block_changed(before: &str, after: &str) {
        assert_eq!(outside(before), outside(after));
        assert!(after.len() > before.len());
    }

    #[test]
    fn vanilla_lacks_addons_and_gets_it_lf() {
        assert!(!VANILLA.contains('\r'));
        assert_eq!(has_addons(VANILLA), Ok(false));
        let out = ensure_addons(VANILLA).unwrap();
        assert_eq!(has_addons(&out), Ok(true));
        assert_only_block_changed(VANILLA, &out);
        assert!(!out.contains('\r'));
        assert_eq!(out.lines().count(), VANILLA.lines().count() + 1);
        let added = out.lines().find(|l| l.contains(ADDONS_LINE_VALUE)).unwrap();
        assert_eq!(added, "            Game citadel/addons");
        crate::gi::validate_braces(&out).expect("balanced braces");
    }

    #[test]
    fn vanilla_crlf_preserved() {
        let src = crlf(VANILLA);
        let out = ensure_addons(&src).unwrap();
        assert_eq!(has_addons(&out), Ok(true));
        assert_only_block_changed(&src, &out);
        assert_eq!(out.matches('\n').count(), out.matches("\r\n").count());
        crate::gi::validate_braces(&out).expect("balanced braces");
    }

    #[test]
    fn inserted_line_sits_between_low_violence_and_game_citadel() {
        let out = ensure_addons(VANILLA).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        let i = lines.iter().position(|l| l.contains(ADDONS_LINE_VALUE)).unwrap();
        assert!(lines[i - 2].contains("Game_LowViolence"));
        assert!(lines[i - 1].trim().is_empty());
        assert_eq!(lines[i + 1].trim(), "Game \"citadel\"");
    }

    #[test]
    fn preset_with_addons_is_noop() {
        assert_eq!(has_addons(PRESET), Ok(true));
        assert_eq!(ensure_addons(PRESET).unwrap(), PRESET);
    }

    #[test]
    fn commented_out_counts_as_absent() {
        let src = "FileSystem\n{\n\tSearchPaths\n\t{\n\t\t// Game citadel/addons\n\t\tMod\t\tcitadel\n\t\tGame\t\tcitadel\n\t}\n}\n";
        assert_eq!(has_addons(src), Ok(false));
        let out = ensure_addons(src).unwrap();
        assert_eq!(
            out,
            "FileSystem\n{\n\tSearchPaths\n\t{\n\t\t// Game citadel/addons\n\t\tMod\t\tcitadel\n\t\tGame\t\t\t\tcitadel/addons\n\t\tGame\t\tcitadel\n\t}\n}\n"
        );
    }

    #[test]
    fn detects_quotes_case_and_trailing_comment() {
        for line in [
            "game \"citadel/addons\" // x",
            "GAME   citadel/addons",
            "Game\t\"citadel/addons\"",
        ] {
            let src = format!("SearchPaths\n{{\n  {line}\n}}\n");
            assert_eq!(has_addons(&src), Ok(true), "{line}");
        }
        let src = "SearchPaths\n{\n  Game citadel/addons2\n  Mod citadel/addons\n}\n";
        assert_eq!(has_addons(src), Ok(false));
    }

    #[test]
    fn missing_and_unbalanced() {
        assert_eq!(has_addons("FileSystem\n{\n}\n"), Err(SearchPathsError::Missing));
        assert_eq!(
            ensure_addons("// SearchPaths\n{ }"),
            Err(SearchPathsError::Missing)
        );
        assert_eq!(
            has_addons("SearchPaths\n{\n  Game citadel\n"),
            Err(SearchPathsError::Unbalanced)
        );
        assert_eq!(
            has_addons("SearchPaths\n  Game citadel\n"),
            Err(SearchPathsError::Unbalanced)
        );
    }

    #[test]
    fn empty_block_gets_default_style() {
        let out = ensure_addons("SearchPaths\n{\n}\n").unwrap();
        assert_eq!(out, "SearchPaths\n{\n    Game\t\t\t\tcitadel/addons\n}\n");
    }

    #[test]
    fn no_plain_game_entry_inserts_before_closing_brace() {
        let src = "SearchPaths\n{\n  Mod  citadel\n  Game_Language  x\n}\n";
        let out = ensure_addons(src).unwrap();
        assert_eq!(
            out,
            "SearchPaths\n{\n  Mod  citadel\n  Game_Language  x\n  Game           citadel/addons\n}\n"
        );
    }

    #[test]
    fn idempotent() {
        for src in [VANILLA.to_owned(), crlf(VANILLA)] {
            let once = ensure_addons(&src).unwrap();
            assert_eq!(ensure_addons(&once).unwrap(), once);
        }
    }
}
