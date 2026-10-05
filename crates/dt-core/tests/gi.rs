//! Golden tests for `dt_core::gi` against the real preset files under `research/configs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dt_core::gi::*;

const SQOOKY: &str = "OptimizationLock/Sqooky's .gi";
const BOOT: &str = "OptimizationLock/boot's maxium fps config";
const KAIZ: &str = "OptimizationLock/kaizuchanerus minimum spec";

fn research(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../research")
        .join(rel)
}

fn fixture(preset_dir: &str) -> String {
    let path = research(&format!("configs/{preset_dir}/gameinfo.gi"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn all_fixtures() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.file_name().is_some_and(|n| n == "gameinfo.gi") {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(&research("configs"), &mut paths);
    paths.sort();
    assert_eq!(
        paths.len(),
        17,
        "fixture set changed; update the expectations in this file"
    );
    paths
        .into_iter()
        .map(|p| (p.clone(), std::fs::read_to_string(&p).unwrap()))
        .collect()
}

fn to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

fn set(value: &str) -> Override {
    Override::Set(value.to_string())
}

fn overrides(pairs: &[(&str, Override)]) -> Overrides {
    pairs
        .iter()
        .map(|(name, o)| (name.to_string(), o.clone()))
        .collect()
}

fn line_starting_with<'a>(text: &'a str, prefix: &str) -> &'a str {
    text.lines()
        .find(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("no line starts with {prefix:?}"))
}

fn differing_lines<'a>(a: &'a str, b: &'a str) -> Vec<(&'a str, &'a str)> {
    let (a, b): (Vec<_>, Vec<_>) = (a.lines().collect(), b.lines().collect());
    assert_eq!(a.len(), b.len(), "line count changed");
    a.into_iter().zip(b).filter(|(x, y)| x != y).collect()
}

fn names(entries: &[ConVarEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn detect_eol_prefers_crlf_when_any_line_has_it() {
    assert_eq!(detect_eol("a\nb\n"), Eol::Lf);
    assert_eq!(detect_eol("a\r\nb\r\n"), Eol::CrLf);
    assert_eq!(detect_eol("a\nb\r\n"), Eol::CrLf);
    assert_eq!(detect_eol(""), Eol::Lf);
}

#[test]
fn empty_overrides_leave_every_fixture_byte_identical_in_both_eol_styles() {
    for (path, text) in all_fixtures() {
        for variant in [text.clone(), to_crlf(&text)] {
            let out = apply_overrides(&variant, &Overrides::new()).unwrap();
            assert!(
                out.text == variant,
                "{} changed under empty overrides",
                path.display()
            );
            assert!(
                out.changed.is_empty() && out.injected.is_empty(),
                "{}",
                path.display()
            );
        }
    }
}

#[test]
fn every_fixture_validates_and_exposes_its_convars() {
    for (path, text) in all_fixtures() {
        validate_braces(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let entries = read_convars(&text).unwrap();
        assert!(
            entries.len() >= 80,
            "{}: only {} convars",
            path.display(),
            entries.len()
        );
        let expected_eol = if path.to_string_lossy().contains("kaizuchanerus") {
            Eol::CrLf
        } else {
            Eol::Lf
        };
        assert_eq!(detect_eol(&text), expected_eol, "{}", path.display());
    }
}

#[test]
fn read_convars_reports_zero_based_line_of_each_entry() {
    let text = fixture(SQOOKY);
    let entries = read_convars(&text).unwrap();
    let farz = entries.iter().find(|e| e.name == "r_farz").unwrap();
    assert!(
        text.lines()
            .nth(farz.line)
            .unwrap()
            .trim_start()
            .starts_with("r_farz")
    );
    assert_eq!(farz.value, "7000");
    assert!(!farz.commented);
    assert_eq!(
        farz.comment.as_deref(),
        Some(r#"// This controls the far clipping plane, ie building/player popin   [def: "-1"]"#)
    );
}

/// The catalog CSV was generated with the same convar regex; every non-empty preset cell
/// must agree with what `read_convars` sees (last occurrence wins, `//` prefix = commented).
/// The CSV did not skip nested blocks, so its `rate { min default max }` and
/// `voice_always_sample_mic { version default }` keys are excluded here.
#[test]
fn read_convars_agrees_with_the_catalog_csv() {
    const NESTED_KEYS: [&str; 4] = ["default", "max", "min", "version"];
    let columns = [
        ("sqooky", SQOOKY, 400),
        ("sqooky_test", "OptimizationLock/test_cfg", 430),
        ("kaiz_minspec", KAIZ, 270),
        ("boot_maxfps", BOOT, 450),
        (
            "optilock_recommended",
            "OptiLock/OptiLock FPS Config (Recommended)",
            540,
        ),
        ("optilock_potato", "OptiLock/OptiLock Potato Config", 520),
    ];
    let mut reader = csv::Reader::from_path(research("data/convar_catalog.csv")).unwrap();
    let headers = reader.headers().unwrap().clone();
    let rows: Vec<csv::StringRecord> = reader.records().map(Result::unwrap).collect();
    for (column, preset_dir, min_rows) in columns {
        let col = headers.iter().position(|h| h == column).unwrap();
        let text = fixture(preset_dir);
        let lines: Vec<&str> = text.lines().collect();
        // The csv's preset columns were generated from unquoted `name value` lines only.
        let entries: Vec<ConVarEntry> = read_convars(&text)
            .unwrap()
            .into_iter()
            .filter(|e| {
                !lines[e.line]
                    .trim_start()
                    .trim_start_matches("//")
                    .trim_start()
                    .starts_with('"')
            })
            .collect();
        let mut checked = 0;
        for row in &rows {
            let (name, cell) = (&row[0], &row[col]);
            if cell.is_empty() || NESTED_KEYS.contains(&name) {
                continue;
            }
            let (commented, value) = match cell.strip_prefix("//") {
                Some(v) => (true, v),
                None => (false, cell),
            };
            let entry = entries
                .iter()
                .rev()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("{column}: {name} missing"));
            assert_eq!(
                (entry.commented, entry.value.as_str()),
                (commented, value),
                "{column}: {name}"
            );
            checked += 1;
        }
        assert!(checked >= min_rows, "{column}: only {checked} rows checked");
    }
}

#[test]
fn r_farz_per_preset() {
    let expected = [
        (SQOOKY, "7000"),
        (KAIZ, "4500"),
        (BOOT, "6000"),
        ("OptiLock/OptiLock FPS Config (Recommended)", "8000"),
    ];
    for (preset_dir, value) in expected {
        assert_eq!(
            effective_values(&fixture(preset_dir)).unwrap()["r_farz"],
            value,
            "{preset_dir}"
        );
    }
}

#[test]
fn effective_values_skip_commented_lines_and_take_the_last_active_duplicate() {
    let values = effective_values(&fixture(SQOOKY)).unwrap();
    assert!(
        !values.contains_key("ai_disable"),
        "ai_disable is commented out in Sqooky's preset"
    );
    assert_eq!(
        values["cl_phys_enabled"], "true",
        "the later commented duplicate must not shadow the active line"
    );
    assert_eq!(values["fps_max"], "400");
}

#[test]
fn set_rewrites_the_value_in_place_and_keeps_padding_and_trailing_comment() {
    let text = fixture(SQOOKY);
    let out = apply_overrides(&text, &overrides(&[("r_farz", set("9000"))])).unwrap();
    let diff = differing_lines(&text, &out.text);
    assert_eq!(
        diff,
        vec![(
            r#"        r_farz       "7000" // This controls the far clipping plane, ie building/player popin   [def: "-1"]"#,
            r#"        r_farz       "9000" // This controls the far clipping plane, ie building/player popin   [def: "-1"]"#,
        )]
    );
    assert_eq!(out.changed, vec!["r_farz"]);
    assert!(out.injected.is_empty());

    let crlf = to_crlf(&text);
    let out = apply_overrides(&crlf, &overrides(&[("r_farz", set("9000"))])).unwrap();
    assert_eq!(
        out.text,
        to_crlf(&out.text),
        "CRLF input must stay pure CRLF"
    );
    assert_eq!(differing_lines(&crlf, &out.text).len(), 1);
}

#[test]
fn quoted_names_are_convars_and_keep_their_quotes_when_rewritten() {
    let text = "ConVars\n{\n\t\"r_ssao\" \"false\"\n\t\"fps_max\"\t\"400\" // cap\n\t\"voice_x\"\n\t{\n\t\t\"version\" \"2\"\n\t}\n\t\"bad name\" \"1\"\n}\n";
    let values = effective_values(text).unwrap();
    assert_eq!(
        values,
        BTreeMap::from([
            ("fps_max".to_string(), "400".to_string()),
            ("r_ssao".to_string(), "false".to_string()),
        ]),
        "nested blocks and names with spaces are not convars"
    );
    let out = apply_overrides(
        text,
        &overrides(&[("fps_max", set("240")), ("r_ssao", Override::Comment)]),
    )
    .unwrap();
    assert_eq!(
        differing_lines(text, &out.text),
        vec![
            ("\t\"r_ssao\" \"false\"", "\t// \"r_ssao\" \"false\""),
            (
                "\t\"fps_max\"\t\"400\" // cap",
                "\t\"fps_max\"\t\"240\" // cap"
            ),
        ]
    );
    assert!(out.injected.is_empty());
    let read = read_convars(&out.text).unwrap();
    assert!(read.iter().any(|e| e.name == "r_ssao" && e.commented));
}

#[test]
fn set_with_the_current_value_changes_nothing() {
    let text = fixture(SQOOKY);
    let out = apply_overrides(&text, &overrides(&[("r_farz", set("7000"))])).unwrap();
    assert_eq!(out.text, text);
    assert!(out.changed.is_empty());
}

#[test]
fn set_on_a_commented_line_uncomments_it() {
    let text = fixture(SQOOKY);
    let before = line_starting_with(&text, "        // ai_disable ");
    let out = apply_overrides(&text, &overrides(&[("ai_disable", set("1"))])).unwrap();
    let diff = differing_lines(&text, &out.text);
    assert_eq!(diff, vec![(before, before.replacen("// ", "", 1).as_str())]);
    assert_eq!(effective_values(&out.text).unwrap()["ai_disable"], "1");
    assert_eq!(out.changed, vec!["ai_disable"]);
}

#[test]
fn comment_prefixes_active_lines_and_leaves_commented_ones_alone() {
    let text = fixture(SQOOKY);
    let out = apply_overrides(&text, &overrides(&[("fps_max", Override::Comment)])).unwrap();
    assert_eq!(
        differing_lines(&text, &out.text),
        vec![(
            r#"        fps_max    "400""#,
            r#"        // fps_max    "400""#
        )]
    );
    assert!(!effective_values(&out.text).unwrap().contains_key("fps_max"));
    assert_eq!(out.changed, vec!["fps_max"]);

    let out = apply_overrides(&text, &overrides(&[("ai_disable", Override::Comment)])).unwrap();
    assert_eq!(out.text, text);
    assert!(out.changed.is_empty());
}

fn mixed_overrides() -> Overrides {
    overrides(&[
        ("r_farz", set("9000")),
        ("ai_disable", set("1")),
        ("fps_max", Override::Comment),
        ("cl_phys_enabled", set("true")),
        ("dt_unknown_a", set("42")),
        ("dt_unknown_b", set("x y")),
    ])
}

#[test]
fn unknown_sets_go_to_a_managed_block_and_reapplying_converges() {
    for (label, text) in [
        ("sqooky lf", fixture(SQOOKY)),
        ("sqooky crlf", to_crlf(&fixture(SQOOKY))),
        ("kaiz crlf", fixture(KAIZ)),
    ] {
        let once = apply_overrides(&text, &mixed_overrides()).unwrap();
        validate_braces(&once.text).unwrap();
        let present = read_convars(&text).unwrap();
        for name in &once.injected {
            assert!(
                !present.iter().any(|e| &e.name == name),
                "{label}: {name} exists in the body, must not be injected"
            );
        }
        for name in ["dt_unknown_a", "dt_unknown_b"] {
            assert!(
                once.injected.contains(&name.to_string()),
                "{label}: {name} not injected"
            );
        }
        assert!(once.changed.contains(&"r_farz".to_string()), "{label}");
        let values = effective_values(&once.text).unwrap();
        assert_eq!(values["r_farz"], "9000", "{label}");
        assert_eq!(values["dt_unknown_a"], "42", "{label}");
        assert_eq!(values["dt_unknown_b"], "x y", "{label}");
        assert!(!values.contains_key("fps_max"), "{label}");
        let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
        assert_eq!(
            once.text.matches('\n').count(),
            once.text.matches(eol).count(),
            "{label}: injected lines must use the file's EOL"
        );

        let twice = apply_overrides(&once.text, &mixed_overrides()).unwrap();
        assert!(
            twice.text == once.text,
            "{label}: second apply must be a no-op"
        );
        assert!(twice.changed.is_empty(), "{label}: {:?}", twice.changed);
        assert_eq!(twice.injected, once.injected, "{label}");

        let fewer =
            apply_overrides(&once.text, &overrides(&[("dt_unknown_a", set("42"))])).unwrap();
        let fewer_names = names(&read_convars(&fewer.text).unwrap())
            .into_iter()
            .filter(|n| n.starts_with("dt_unknown"))
            .count();
        assert_eq!(
            fewer_names, 1,
            "{label}: the managed block is regenerated, not appended to"
        );

        let cleared = apply_overrides(&once.text, &Overrides::new()).unwrap();
        assert!(
            !cleared.text.contains("dt_unknown"),
            "{label}: empty overrides drop the managed block"
        );
    }
}

#[test]
fn managed_block_uses_the_body_indent() {
    let sqooky =
        apply_overrides(&fixture(SQOOKY), &overrides(&[("dt_unknown_a", set("42"))])).unwrap();
    assert!(
        sqooky.text.contains("\n        dt_unknown_a \"42\"\n"),
        "8-space indent like the rest of Sqooky's block"
    );
    let kaiz = apply_overrides(&fixture(KAIZ), &overrides(&[("dt_unknown_a", set("42"))])).unwrap();
    assert!(
        kaiz.text.contains("\r\ndt_unknown_a \"42\"\r\n"),
        "kaiz convars are flush left"
    );
}

#[test]
fn replace_convars_block_keeps_everything_outside_the_block_byte_identical() {
    const HEADER: &str = "    ConVars\n    {\n";
    const FOOTER: &str = "\n    }\n\n    Memory";
    let target = fixture(SQOOKY).replacen(
        "            Mod                 citadel\n",
        "            Game                citadel/addons/dmm\n            Mod                 citadel\n",
        1,
    );
    assert!(
        target.contains("citadel/addons/dmm"),
        "fixture edit must apply"
    );
    let source = fixture(BOOT);
    let body_of = |text: &str| {
        let start = text.find(HEADER).unwrap() + HEADER.len();
        let end = text.rfind(FOOTER).unwrap();
        text[start..end].to_string()
    };
    let expected = format!(
        "{}{}{}",
        &target[..target.find(HEADER).unwrap() + HEADER.len()],
        body_of(&source),
        &target[target.rfind(FOOTER).unwrap()..]
    );

    let out = replace_convars_block(&target, &source).unwrap();
    assert!(out == expected, "LF target");
    assert!(out.contains("citadel/addons/dmm"));
    assert_eq!(
        effective_values(&out).unwrap(),
        effective_values(&source).unwrap()
    );

    let out = replace_convars_block(&to_crlf(&target), &source).unwrap();
    assert!(
        out == to_crlf(&expected),
        "CRLF target takes the source body in its own EOL style"
    );
}

#[test]
fn truncated_files_are_rejected_before_anything_is_written() {
    let text = fixture(SQOOKY);
    let mut cut = text.len() * 6 / 10;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let truncated = &text[..cut];
    assert!(matches!(
        validate_braces(truncated),
        Err(GiError::UnbalancedBraces { .. })
    ));
    assert!(apply_overrides(truncated, &overrides(&[("r_farz", set("1"))])).is_err());
    assert!(apply_overrides(truncated, &Overrides::new()).is_err());
    assert!(read_convars(truncated).is_err());
    assert!(replace_convars_block(&text, truncated).is_err());
    assert!(replace_convars_block(truncated, &text).is_err());
}

#[test]
fn missing_convars_block_is_an_error() {
    let text = "\"GameInfo\"\n{\n    game \"citadel\"\n}\n";
    assert_eq!(validate_braces(text), Ok(()));
    assert_eq!(read_convars(text).unwrap_err(), GiError::NoConVarsBlock);
    assert_eq!(
        apply_overrides(text, &Overrides::new()).unwrap_err(),
        GiError::NoConVarsBlock
    );
    assert_eq!(
        replace_convars_block(text, &fixture(SQOOKY)).unwrap_err(),
        GiError::NoConVarsBlock
    );
}

#[test]
fn braces_inside_comments_and_quoted_values_do_not_count() {
    let text = "\"GameInfo\"\n{\n    // ConVars { in a comment must not start the block\n    ConVars\n    {\n        // a stray } here\n        open  \"{\"\n        close \"}\" // and } again\n    }\n}\n";
    assert_eq!(validate_braces(text), Ok(()));
    let entries = read_convars(text).unwrap();
    assert_eq!(names(&entries), vec!["open", "close"]);
    assert_eq!(entries[0].value, "{");
    assert_eq!(entries[1].comment.as_deref(), Some("// and } again"));
    assert_eq!(
        validate_braces("ConVars\n{\n    x \"1\"\n}\n}\n"),
        Err(GiError::UnbalancedBraces { line: 5 })
    );
}

#[test]
fn line_shapes_from_the_wild() {
    let text = concat!(
        "ConVars\n",
        "{\n",
        "\t//nospace_commented \"1\"\n",
        "\tunquoted 4500 // bare number\n",
        "\tspaced\t\"a b\"\t// tab padded\n",
        "\tinline \"http://x\"\n",
        "\trate\n",
        "\t{\n",
        "\t\tmin \"1\"\n",
        "\t}\n",
        "\tlast \"0\"\n",
        "}",
    );
    let entries = read_convars(text).unwrap();
    assert_eq!(
        names(&entries),
        vec!["nospace_commented", "unquoted", "spaced", "inline", "last"]
    );
    assert!(entries[0].commented);
    assert_eq!(entries[0].value, "1");
    assert_eq!(entries[1].value, "4500");
    assert_eq!(entries[1].comment.as_deref(), Some("// bare number"));
    assert_eq!(entries[2].value, "a b");
    assert_eq!(entries[3].value, "http://x");
    assert_eq!(entries[3].comment, None);

    let out = apply_overrides(
        text,
        &overrides(&[
            ("nospace_commented", set("2")),
            ("unquoted", set("5000")),
            ("spaced", Override::Comment),
            ("last", set("1")),
        ]),
    )
    .unwrap();
    assert_eq!(
        out.text,
        concat!(
            "ConVars\n",
            "{\n",
            "\tnospace_commented \"2\"\n",
            "\tunquoted \"5000\" // bare number\n",
            "\t// spaced\t\"a b\"\t// tab padded\n",
            "\tinline \"http://x\"\n",
            "\trate\n",
            "\t{\n",
            "\t\tmin \"1\"\n",
            "\t}\n",
            "\tlast \"1\"\n",
            "}",
        ),
        "no trailing newline is added to a file that had none"
    );
    assert_eq!(
        out.changed,
        vec!["nospace_commented", "unquoted", "spaced", "last"]
    );
}

#[test]
fn set_inside_a_nested_block_does_not_reach_it() {
    let text = "ConVars\n{\n    rate\n    {\n        min \"1\"\n    }\n}\n";
    let out = apply_overrides(text, &overrides(&[("min", set("2"))])).unwrap();
    assert!(
        out.text.contains("        min \"1\"\n"),
        "nested value untouched"
    );
    assert_eq!(out.injected, vec!["min"]);
    let values: BTreeMap<_, _> = effective_values(&out.text).unwrap();
    assert_eq!(
        values["min"], "2",
        "the managed copy is what the engine sees at ConVars depth"
    );
}
