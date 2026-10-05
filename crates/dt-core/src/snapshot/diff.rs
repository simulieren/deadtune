//! Two snapshots compared: added, removed and changed files per category (by CRC), a
//! unified diff of the decoded text for changed files, the DeadTune features that read
//! each changed file, and a count of what changed elsewhere in the game from `pak01.tsv`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::spec::{Category, Source};
use super::store::{self, FileEntry, Manifest};
use super::{SnapshotError, features_of};
use crate::backup::atomic_write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

impl ChangeKind {
    pub fn label(self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Removed => "removed",
            ChangeKind::Changed => "changed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Counts {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
}

impl Counts {
    fn bump(&mut self, kind: ChangeKind) {
        match kind {
            ChangeKind::Added => self.added += 1,
            ChangeKind::Removed => self.removed += 1,
            ChangeKind::Changed => self.changed += 1,
        }
    }

    pub fn total(self) -> usize {
        self.added + self.removed + self.changed
    }

    /// "3 changed, 1 added" (zeros left out), or "no changes".
    pub fn phrase(self) -> String {
        let parts: Vec<String> = [
            (self.changed, "changed"),
            (self.added, "added"),
            (self.removed, "removed"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if parts.is_empty() {
            "no changes".into()
        } else {
            parts.join(", ")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    pub categories: Vec<Category>,
    pub old_size: Option<u64>,
    pub new_size: Option<u64>,
    /// Labels of the DeadTune features that read this file.
    pub features: Vec<String>,
    /// Text file paths relative to each snapshot's `text/`, when decoded on that side.
    pub old_text: Option<String>,
    pub new_text: Option<String>,
    /// Unified diff of the decoded text, for a changed file decoded on both sides.
    pub text_diff: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotDiff {
    pub old: String,
    pub new: String,
    pub old_buildid: Option<String>,
    pub new_buildid: Option<String>,
    pub old_folder: PathBuf,
    pub new_folder: PathBuf,
    pub compared: DateTime<Utc>,
    pub categories: BTreeMap<Category, Counts>,
    pub files: Vec<FileChange>,
    /// Pak entries outside both snapshots, by top folder.
    pub elsewhere: BTreeMap<String, Counts>,
}

impl SnapshotDiff {
    pub fn total(&self) -> Counts {
        let mut c = Counts::default();
        for f in &self.files {
            c.bump(f.kind);
        }
        c
    }

    pub fn elsewhere_total(&self) -> Counts {
        self.elsewhere.values().fold(Counts::default(), |mut c, n| {
            c.added += n.added;
            c.removed += n.removed;
            c.changed += n.changed;
            c
        })
    }

    /// Changes to files a DeadTune feature reads.
    pub fn impact(&self) -> impl Iterator<Item = &FileChange> {
        self.files.iter().filter(|f| !f.features.is_empty())
    }

    pub fn files_of(&self, kind: ChangeKind) -> impl Iterator<Item = &FileChange> {
        self.files.iter().filter(move |f| f.kind == kind)
    }

    /// "Build 100 to 101" or the folder names.
    pub fn title(&self) -> String {
        match (&self.old_buildid, &self.new_buildid) {
            (Some(a), Some(b)) => format!("Build {a} to {b}"),
            _ => format!("{} to {}", self.old, self.new),
        }
    }

    /// `diff-<old>-to-<new>`, by build id when both have one.
    pub fn file_stem(&self) -> String {
        match (&self.old_buildid, &self.new_buildid) {
            (Some(a), Some(b)) => format!("diff-{a}-to-{b}"),
            _ => format!("diff-{}-to-{}", self.old, self.new),
        }
    }

    pub fn report_path(&self) -> PathBuf {
        self.new_folder.join(format!("{}.md", self.file_stem()))
    }

    /// The report without the per-file diffs: what the Copy summary button copies.
    pub fn summary(&self) -> String {
        let mut out = format!(
            "{}: {} in the files DeadTune watches.\n",
            self.title(),
            self.total().phrase()
        );
        let impact: Vec<&FileChange> = self.impact().collect();
        if impact.is_empty() {
            out.push_str("Nothing a DeadTune feature depends on changed.\n");
        } else {
            out.push_str("DeadTune impact:\n");
            for f in impact {
                out.push_str(&format!(
                    "  {} ({}): {}\n",
                    f.features.join(", "),
                    f.kind.label(),
                    f.path
                ));
            }
        }
        for (cat, counts) in &self.categories {
            if counts.total() > 0 {
                out.push_str(&format!("{}: {}\n", cat.label(), counts.phrase()));
            }
        }
        let elsewhere = self.elsewhere_total();
        if elsewhere.total() > 0 {
            let parts: Vec<String> = self
                .elsewhere
                .iter()
                .map(|(dir, c)| format!("{dir} {}", c.phrase()))
                .collect();
            out.push_str(&format!(
                "Elsewhere in the game: {} ({}).\n",
                elsewhere.phrase(),
                parts.join("; ")
            ));
        }
        out
    }

    pub fn markdown(&self) -> String {
        let mut out = format!("# Game files: {}\n\n", self.title());
        out.push_str(&format!(
            "Compared {}. Old: `{}`. New: `{}`.\n\n",
            self.compared.format("%Y-%m-%d %H:%M UTC"),
            self.old_folder.display(),
            self.new_folder.display()
        ));
        out.push_str("## DeadTune impact\n\n");
        let impact: Vec<&FileChange> = self.impact().collect();
        if impact.is_empty() {
            out.push_str("Nothing a DeadTune feature depends on changed.\n\n");
        } else {
            for f in impact {
                out.push_str(&format!(
                    "- **{}**: `{}` {}\n",
                    f.features.join("**, **"),
                    f.path,
                    f.kind.label()
                ));
            }
            out.push('\n');
        }
        out.push_str(
            "## By category\n\n| Category | Changed | Added | Removed |\n|---|---|---|---|\n",
        );
        for (cat, c) in &self.categories {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                cat.label(),
                c.changed,
                c.added,
                c.removed
            ));
        }
        out.push('\n');
        for (kind, heading) in [
            (ChangeKind::Changed, "Changed files"),
            (ChangeKind::Added, "Added files"),
            (ChangeKind::Removed, "Removed files"),
        ] {
            let files: Vec<&FileChange> = self.files_of(kind).collect();
            if files.is_empty() {
                continue;
            }
            out.push_str(&format!("## {heading}\n\n"));
            for f in &files {
                let size = match (f.old_size, f.new_size) {
                    (Some(a), Some(b)) if a != b => format!(" ({a} to {b} bytes)"),
                    (Some(a), _) | (_, Some(a)) => format!(" ({a} bytes)"),
                    _ => String::new(),
                };
                out.push_str(&format!("- `{}`{size}\n", f.path));
            }
            out.push('\n');
        }
        let elsewhere = self.elsewhere_total();
        out.push_str("## Elsewhere in the game\n\n");
        if elsewhere.total() == 0 {
            out.push_str("No other pak01 entries changed.\n\n");
        } else {
            out.push_str(&format!("{} outside the snapshot:\n\n", elsewhere.phrase()));
            for (dir, c) in &self.elsewhere {
                out.push_str(&format!("- {dir}: {}\n", c.phrase()));
            }
            out.push('\n');
        }
        let with_diff: Vec<&FileChange> = self
            .files
            .iter()
            .filter(|f| f.text_diff.is_some())
            .collect();
        if !with_diff.is_empty() {
            out.push_str("## Text diffs\n\n");
            for f in with_diff {
                out.push_str(&format!(
                    "### `{}`\n\n```diff\n{}```\n\n",
                    f.path,
                    f.text_diff.as_deref().unwrap_or_default()
                ));
            }
        }
        out
    }
}

fn text_of(folder: &Path, entry: &FileEntry) -> Option<String> {
    let path = entry.text_path(folder)?;
    std::fs::read_to_string(path).ok()
}

fn unified(old: &str, new: &str, path: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .header(&format!("old/{path}"), &format!("new/{path}"))
        .to_string()
}

/// `path -> (size, crc)` from a `pak01.tsv`; missing or unreadable gives `None`.
fn pak_list(folder: &Path) -> Option<BTreeMap<String, (u64, String)>> {
    let text = std::fs::read_to_string(folder.join(store::PAK_LIST)).ok()?;
    Some(
        text.lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');
                let path = parts.next()?;
                let size = parts.next()?.parse().ok()?;
                let crc = parts.next()?;
                Some((path.to_string(), (size, crc.to_string())))
            })
            .collect(),
    )
}

fn top_folder(path: &str) -> String {
    path.split_once('/')
        .map_or(path, |(dir, _)| dir)
        .to_string()
}

pub fn compare(old_folder: &Path, new_folder: &Path) -> Result<SnapshotDiff, SnapshotError> {
    let old = Manifest::load(old_folder)?;
    let new = Manifest::load(new_folder)?;
    let name = |folder: &Path| {
        folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let old_by: BTreeMap<&str, &FileEntry> =
        old.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let new_by: BTreeMap<&str, &FileEntry> =
        new.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let paths: BTreeSet<&str> = old_by.keys().chain(new_by.keys()).copied().collect();

    let mut categories: BTreeMap<Category, Counts> = Category::ALL
        .into_iter()
        .map(|c| (c, Counts::default()))
        .collect();
    let mut files = Vec::new();
    for path in paths {
        let (before, after) = (old_by.get(path).copied(), new_by.get(path).copied());
        let kind = match (before, after) {
            (None, Some(_)) => ChangeKind::Added,
            (Some(_), None) => ChangeKind::Removed,
            (Some(a), Some(b)) if a.crc != b.crc => ChangeKind::Changed,
            _ => continue,
        };
        let entry = after.or(before).expect("one side exists");
        // The appmanifest changes with every build; the build ids are the report's title.
        if entry.source == Source::Steam {
            continue;
        }
        for cat in &entry.categories {
            categories.entry(*cat).or_default().bump(kind);
        }
        let text_diff = match (before, after) {
            (Some(a), Some(b)) => match (text_of(old_folder, a), text_of(new_folder, b)) {
                (Some(x), Some(y)) => Some(unified(&x, &y, path)),
                _ => None,
            },
            _ => None,
        };
        files.push(FileChange {
            path: path.to_string(),
            kind,
            categories: entry.categories.clone(),
            old_size: before.map(|e| e.size),
            new_size: after.map(|e| e.size),
            features: features_of(path).into_iter().map(|f| f.label()).collect(),
            old_text: before.and_then(|e| e.text.clone()),
            new_text: after.and_then(|e| e.text.clone()),
            text_diff,
        });
    }

    let mut elsewhere: BTreeMap<String, Counts> = BTreeMap::new();
    if let (Some(old_list), Some(new_list)) = (pak_list(old_folder), pak_list(new_folder)) {
        let listed: BTreeSet<&str> = old_by.keys().chain(new_by.keys()).copied().collect();
        let all: BTreeSet<&String> = old_list.keys().chain(new_list.keys()).collect();
        for path in all {
            if listed.contains(path.as_str()) {
                continue;
            }
            let kind = match (old_list.get(path), new_list.get(path)) {
                (None, Some(_)) => ChangeKind::Added,
                (Some(_), None) => ChangeKind::Removed,
                (Some(a), Some(b)) if a.1 != b.1 => ChangeKind::Changed,
                _ => continue,
            };
            elsewhere.entry(top_folder(path)).or_default().bump(kind);
        }
    }

    Ok(SnapshotDiff {
        old: name(old_folder),
        new: name(new_folder),
        old_buildid: old.buildid,
        new_buildid: new.buildid,
        old_folder: old_folder.to_path_buf(),
        new_folder: new_folder.to_path_buf(),
        compared: Utc::now(),
        categories,
        files,
        elsewhere,
    })
}

/// Writes the Markdown and TOML reports into the newer snapshot; returns the Markdown path.
pub fn write(diff: &SnapshotDiff) -> Result<PathBuf, SnapshotError> {
    let md = diff.report_path();
    atomic_write(&md, diff.markdown().as_bytes())?;
    let toml = toml::to_string(diff).map_err(|e| SnapshotError::Toml(e.to_string()))?;
    atomic_write(&md.with_extension("toml"), toml.as_bytes())?;
    Ok(md)
}

/// The newest report in a snapshot folder.
pub fn load_latest(folder: &Path) -> Option<SnapshotDiff> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("diff-") && n.ends_with(".toml"))
        .collect();
    names.sort();
    let text = std::fs::read_to_string(folder.join(names.last()?)).ok()?;
    toml::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;

    use super::*;
    use crate::hud::elements::HUD_STYLE;
    use crate::hud::inject;
    use crate::hud::topbar::TOP_BAR_LAYOUT;
    use crate::hud::vpk;
    use crate::snapshot::export::tests::{fake_files, fake_install, write_manifest};
    use crate::snapshot::{Selection, export, store};

    fn go(_: export::Progress) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    /// Two builds: the top bar layout rebuilt, a HUD script added, the clock script removed,
    /// a model changed and a map added outside the snapshot.
    fn two_snapshots() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let (tmp, paths) = fake_install(&vpk::write(&fake_files()), "100");
        let data = tmp.path().join("data");
        let first = export::take(&paths, &data, &Selection::default(), &mut go).unwrap();

        let mut files = fake_files();
        files.insert(
            TOP_BAR_LAYOUT.to_string(),
            inject::patched_layout(
                &files[TOP_BAR_LAYOUT],
                &inject::LayoutEdit {
                    style_includes: vec!["s2r://panorama/styles/deadtune/x.vcss_c".into()],
                    ..inject::LayoutEdit::default()
                },
                &[],
                "",
            )
            .unwrap(),
        );
        files.insert(
            "panorama/scripts/hud_new.vjs_c".to_string(),
            inject::script_resource("new"),
        );
        files.remove("panorama/scripts/hud_clock.vjs_c");
        files.insert("models/heroes/x/x.vmdl_c".to_string(), vec![9, 9, 9]);
        files.insert("maps/dl_new.vpk".to_string(), vec![1]);
        std::fs::write(
            paths.citadel_dir.join(crate::hud::install::GAME_PAK),
            vpk::write(&files),
        )
        .unwrap();
        write_manifest(&tmp.path().join("steamapps"), "101");
        let paths = crate::locate::from_game_root(&paths.game_root).unwrap();
        let second = export::take(&paths, &data, &Selection::default(), &mut go).unwrap();
        (tmp, first.folder, second.folder)
    }

    #[test]
    fn compare_finds_added_removed_and_changed_and_flags_deadtune_files() {
        let (tmp, old, new) = two_snapshots();
        let diff = compare(&old, &new).unwrap();
        assert_eq!(diff.old_buildid.as_deref(), Some("100"));
        assert_eq!(diff.new_buildid.as_deref(), Some("101"));
        assert_eq!(diff.title(), "Build 100 to 101");
        assert_eq!(diff.file_stem(), "diff-100-to-101");
        let kinds: Vec<(&str, ChangeKind)> = diff
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                (TOP_BAR_LAYOUT, ChangeKind::Changed),
                ("panorama/scripts/hud_clock.vjs_c", ChangeKind::Removed),
                ("panorama/scripts/hud_new.vjs_c", ChangeKind::Added),
            ]
        );
        assert_eq!(
            diff.total(),
            Counts {
                added: 1,
                removed: 1,
                changed: 1
            }
        );
        assert_eq!(
            diff.categories[&Category::Hud],
            Counts {
                added: 1,
                removed: 1,
                changed: 1
            }
        );
        assert_eq!(diff.categories[&Category::Deadtune].changed, 1);
        assert_eq!(diff.categories[&Category::Config], Counts::default());

        let impact: Vec<&FileChange> = diff.impact().collect();
        assert_eq!(impact.len(), 1);
        assert_eq!(impact[0].path, TOP_BAR_LAYOUT);
        assert_eq!(impact[0].features, ["Top bar"]);
        let text = impact[0].text_diff.as_deref().unwrap();
        assert!(text.starts_with("--- old/panorama/layout/citadel_hud_top_bar.vxml_c\n+++ new/"));
        assert!(text.contains("+\t\t<include src=\"s2r://panorama/styles/deadtune/x.vcss_c\" />"));
        assert_eq!(
            impact[0].new_text.as_deref(),
            Some("panorama/layout/citadel_hud_top_bar.xml")
        );
        assert!(
            diff.files[1].text_diff.is_none(),
            "removed files carry no diff"
        );

        assert_eq!(
            diff.elsewhere,
            BTreeMap::from([
                (
                    "maps".to_string(),
                    Counts {
                        added: 1,
                        ..Counts::default()
                    }
                ),
                (
                    "models".to_string(),
                    Counts {
                        changed: 1,
                        ..Counts::default()
                    }
                ),
            ])
        );

        let summary = diff.summary();
        assert!(
            summary.starts_with("Build 100 to 101: 1 changed, 1 added, 1 removed"),
            "{summary}"
        );
        assert!(summary.contains("Top bar (changed): panorama/layout/citadel_hud_top_bar.vxml_c"));
        assert!(summary.contains("HUD: 1 changed, 1 added, 1 removed"));
        assert!(summary.contains(
            "Elsewhere in the game: 1 changed, 1 added (maps 1 added; models 1 changed)."
        ));
        assert!(!summary.contains("```"));

        let md = diff.markdown();
        assert!(md.starts_with("# Game files: Build 100 to 101\n"));
        assert!(md.contains("- **Top bar**: `panorama/layout/citadel_hud_top_bar.vxml_c` changed"));
        assert!(md.contains("| HUD | 1 | 1 | 1 |"));
        assert!(md.contains("## Removed files\n\n- `panorama/scripts/hud_clock.vjs_c`"));
        assert!(md.contains("```diff\n--- old/"));

        let report = write(&diff).unwrap();
        assert_eq!(report, new.join("diff-100-to-101.md"));
        assert_eq!(std::fs::read_to_string(&report).unwrap(), md);
        assert_eq!(load_latest(&new), Some(diff.clone()));
        assert_eq!(load_latest(&old), None);
        let info = store::find(&store::dir(&tmp.path().join("data")), "latest").unwrap();
        assert_eq!(info.reports, ["diff-100-to-101.md"]);
    }

    #[test]
    fn identical_snapshots_report_nothing() {
        let (tmp, paths) = fake_install(&vpk::write(&fake_files()), "100");
        let data = tmp.path().join("data");
        let first = export::take(&paths, &data, &Selection::default(), &mut go).unwrap();
        write_manifest(&tmp.path().join("steamapps"), "101");
        let paths = crate::locate::from_game_root(&paths.game_root).unwrap();
        let second = export::take(&paths, &data, &Selection::default(), &mut go).unwrap();
        let diff = compare(&first.folder, &second.folder).unwrap();
        assert!(diff.files.is_empty());
        assert!(diff.impact().next().is_none());
        assert!(diff.elsewhere.is_empty());
        assert_eq!(
            diff.summary(),
            "Build 100 to 101: no changes in the files DeadTune watches.\nNothing a DeadTune feature depends on changed.\n"
        );
        assert!(diff.markdown().contains("No other pak01 entries changed."));
        assert!(matches!(
            compare(&first.folder, tmp.path()),
            Err(SnapshotError::NotASnapshot(_))
        ));
        let _ = HUD_STYLE;
    }
}
