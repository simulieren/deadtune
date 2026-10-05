//! Files coming into the UI images page and collections going out. A dropped or chosen
//! file replaces the selected image; several files, a folder or a zip are matched to game
//! images by name (`dt_core::hud::collection`) and wait for the player to confirm. Going
//! out: the picked images as the game has them, or every change as one zip of finished
//! pictures that imports back here or on someone else's DeadTune.

use std::path::{Path, PathBuf};

use dt_core::hud::collection::{self, PackFile, Plan};
use dt_core::hud::icons::{self, Source};
use dt_core::snapshot::images::Found;
use dt_core::texture::encode::{Fit, MAX_SIDE};
use dt_core::texture::{png, svg};
use dt_core::zip::{self, ZipWriter};

use crate::images::{ExportKind, ImageSource, Tone, plain_error, render};
use crate::state::AppState;

/// Something dropped on the window or picked in the file dialog.
#[derive(Debug)]
pub enum Incoming {
    File {
        name: String,
        bytes: Result<Vec<u8>, String>,
    },
    Folder(PathBuf),
}

impl Incoming {
    pub fn from_path(path: &Path) -> Incoming {
        if path.is_dir() {
            return Incoming::Folder(path.to_path_buf());
        }
        Incoming::File {
            name: file_name(path),
            bytes: std::fs::read(path).map_err(|e| e.to_string()),
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A collection matched against the game, waiting for "Import".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReview {
    pub title: String,
    pub plan: Plan,
    /// How many of the matched images the player already changed.
    pub replaces_changes: usize,
}

fn is_zip(name: &str, bytes: &[u8]) -> bool {
    name.to_lowercase().ends_with(".zip") || zip::is_zip(bytes)
}

impl AppState {
    /// One file replaces the selected image; anything more is a collection to review.
    pub fn receive_files(&mut self, items: Vec<Incoming>) {
        let mut pack = Vec::new();
        let mut titles = Vec::new();
        let mut loose = Vec::new();
        for item in items {
            match item {
                Incoming::Folder(dir) => {
                    titles.push(file_name(&dir));
                    pack.extend(collection::read_folder(&dir));
                }
                Incoming::File {
                    name,
                    bytes: Ok(bytes),
                } if is_zip(&name, &bytes) => match zip::files(&bytes) {
                    Ok(files) => {
                        titles.push(name.trim_end_matches(".zip").to_string());
                        pack.extend(
                            files
                                .into_iter()
                                .map(|(name, bytes)| PackFile { name, bytes }),
                        );
                    }
                    Err(_) => self.page_notice(
                        Tone::Bad,
                        format!("{name} couldn't be read as a zip. Download it again, or unzip it and drop the folder."),
                    ),
                },
                Incoming::File { name, bytes } => loose.push((name, bytes)),
            }
        }
        if pack.is_empty() && titles.is_empty() && loose.len() == 1 {
            let (name, bytes) = loose.pop().expect("one file");
            match (self.images.selected.clone(), bytes) {
                (Some(path), Ok(bytes)) => self.replace_image(&path, &bytes),
                (Some(path), Err(_)) => self.notice(
                    &path,
                    Tone::Bad,
                    "Couldn't read that file. Copy it to a normal folder and try again.",
                ),
                (None, Ok(bytes)) => {
                    self.review_import(name.clone(), vec![PackFile { name, bytes }])
                }
                (None, Err(_)) => self.page_notice(
                    Tone::Bad,
                    "Couldn't read that file. Copy it to a normal folder and try again.",
                ),
            }
            return;
        }
        let unread = loose.iter().filter(|(_, b)| b.is_err()).count();
        pack.extend(loose.into_iter().filter_map(|(name, bytes)| {
            Some(PackFile {
                name,
                bytes: bytes.ok()?,
            })
        }));
        if pack.is_empty() {
            if unread > 0 {
                self.page_notice(
                    Tone::Bad,
                    "Couldn't read those files. Copy them to a normal folder and try again.",
                );
            }
            return;
        }
        let title = match titles.as_slice() {
            [one] => one.clone(),
            _ => format!("{} files", pack.len()),
        };
        self.review_import(title, pack);
    }

    fn review_import(&mut self, title: String, files: Vec<PackFile>) {
        let Some(library) = self.image_library() else {
            return;
        };
        let paths: Vec<String> = library.entries.iter().map(|e| e.path.clone()).collect();
        let plan = collection::plan(files, &paths);
        if plan.matched.is_empty() {
            self.page_notice(
                Tone::Bad,
                "None of these files is named after a game image. Pick an image and drop one file on it, or name the files like the game's (minimap/hero_ally_psd.png).",
            );
            return;
        }
        let replaces_changes = plan
            .matched
            .iter()
            .filter(|m| self.image_override(&m.game_path).is_some())
            .count();
        self.images.import = Some(ImportReview {
            title,
            plan,
            replaces_changes,
        });
    }

    /// Replaces every matched image as one undo step and shows what changed.
    pub fn confirm_import(&mut self) {
        let Some(review) = self.images.import.take() else {
            return;
        };
        let before = self.begin_edit();
        let mut failed = Vec::new();
        for m in &review.plan.matched {
            if let Err(e) = icons::set(
                &mut self.profile.hud.icons,
                &self.store.root,
                &m.game_path,
                &m.file.bytes,
                Fit::Original,
            ) {
                failed.push((m.file.name.clone(), plain_error(&e)));
            }
        }
        self.end_edit(before);
        let done = review.plan.matched.len() - failed.len();
        if done > 0 {
            self.images.folder = None;
            self.images.search.clear();
            self.images.changed_only = true;
            self.clear_marks();
        }
        let mut text = format!(
            "Imported {done} {} from {}. Press Apply to use them in game.",
            plural(done, "image", "images"),
            review.title
        );
        if let Some((name, why)) = failed.first() {
            text.push_str(&format!(
                " {} couldn't be used; {name}: {why}",
                failed.len()
            ));
        }
        let tone = if failed.is_empty() {
            Tone::Good
        } else {
            Tone::Warn
        };
        self.page_notice(tone, text);
    }

    pub fn cancel_import(&mut self) {
        self.images.import = None;
    }

    /// Saves each of `paths` as the game has it into the exports folder.
    pub fn export_images(&mut self, paths: &[String]) {
        let mut saved = 0;
        let mut failed = 0;
        for path in paths {
            let kind = if path.ends_with(".vsvg_c") {
                ExportKind::Svg
            } else {
                ExportKind::Png
            };
            match self.write_export(path, kind) {
                Ok(_) => saved += 1,
                Err(_) => failed += 1,
            }
        }
        self.images.exported = Some(self.exports_dir());
        let mut text = format!(
            "Saved {saved} {} to {}.",
            plural(saved, "image", "images"),
            self.exports_dir().display()
        );
        if failed > 0 {
            text.push_str(&format!(" {failed} couldn't be read."));
        }
        let tone = if failed == 0 { Tone::Good } else { Tone::Warn };
        self.page_notice(tone, text);
    }

    /// Every changed image, finished (placed, recoloured), in one zip under its game path:
    /// a collection that imports back on this page.
    pub fn export_changes(&mut self) -> Result<PathBuf, String> {
        self.commit_edits();
        self.cache_slots();
        let result = self.write_changes();
        match &result {
            Ok((file, count)) => {
                self.images.exported = Some(file.clone());
                self.page_notice(
                    Tone::Good,
                    format!(
                        "Saved your {count} changed {} as {}. Drop that zip here (or on a friend's DeadTune) to import it.",
                        plural(*count, "image", "images"),
                        file.display()
                    ),
                );
            }
            Err(e) => self.page_notice(Tone::Bad, format!("Couldn't save your changes ({e}).")),
        }
        result.map(|(file, _)| file)
    }

    fn write_changes(&self) -> Result<(PathBuf, usize), String> {
        let source = self
            .image_library()
            .map(|l| l.source.clone())
            .ok_or("the images are not loaded")?;
        let mut zip = ZipWriter::new(Vec::new(), chrono::Local::now().naive_local());
        let mut count = 0;
        for path in self.profile.hud.icons.keys() {
            let Ok((name, bytes)) = self.finished_file(&source, path) else {
                continue;
            };
            zip.add(&name, &bytes).map_err(|e| e.to_string())?;
            count += 1;
        }
        if count == 0 {
            return Err("nothing to save yet".into());
        }
        let bytes = zip.finish().map_err(|e| e.to_string())?;
        let dir = self.exports_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stem = format!("my-ui-images-{}", chrono::Local::now().format("%Y-%m-%d"));
        let file = (1..)
            .map(|n| match n {
                1 => dir.join(format!("{stem}.zip")),
                n => dir.join(format!("{stem}-{n}.zip")),
            })
            .find(|f| !f.exists())
            .expect("a free name");
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        Ok((file, count))
    }

    /// The changed image at `path` as the game will draw it, named for a collection.
    fn finished_file(&self, source: &ImageSource, path: &str) -> Result<(String, Vec<u8>), String> {
        let over = self.image_override(path).ok_or("not changed")?;
        let vector_text = match (&over.source, path.ends_with(".vsvg_c")) {
            (Source::Svg { .. }, _) => {
                let file = self.stored_image(path).ok_or("no stored copy")?;
                Some(std::fs::read_to_string(file).map_err(|e| e.to_string())?)
            }
            (Source::Game, true) => Some(game_svg(source, path)?),
            _ => None,
        };
        if let Some(text) = vector_text {
            let text = svg::adjust(&text, &over.adjust);
            return Ok((collection::file_name(path, "svg"), text.into_bytes()));
        }
        let shown = render(source, &self.preview_picture(path, MAX_SIDE))?;
        let bytes = png::write(&shown.image).map_err(|e| e.to_string())?;
        Ok((collection::file_name(path, "png"), bytes))
    }

    /// Picks up the file dialog's answer once it closes.
    pub fn poll_picker(&mut self) {
        let Some(result) = self.images.picker.as_ref().and_then(|p| p.poll()) else {
            return;
        };
        self.images.picker = None;
        match result {
            Ok(paths) if paths.is_empty() => {}
            Ok(paths) => {
                let items = paths.iter().map(|p| Incoming::from_path(p)).collect();
                self.receive_files(items);
            }
            Err(e) => self.page_notice(Tone::Bad, e),
        }
    }
}

fn game_svg(source: &ImageSource, path: &str) -> Result<String, String> {
    match source.find(path)? {
        Found::Compiled(bytes) => svg::svg_text(&bytes).map_err(|e| e.to_string()),
        Found::Decoded(file) => std::fs::read_to_string(file).map_err(|e| e.to_string()),
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::images::tests::{GAME_SVG, ITEM, TEXTURE, TOP_BAR, loaded, my_png};
    use dt_core::hud::collection::Skip;
    use dt_core::hud::icons::IconOverride;
    use dt_core::texture::adjust::Adjust;
    use dt_core::texture::frame::Crop;

    const MY_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\" fill=\"#ff0000\"/></svg>";

    fn file(name: &str, bytes: Vec<u8>) -> Incoming {
        Incoming::File {
            name: name.into(),
            bytes: Ok(bytes),
        }
    }

    #[test]
    fn one_file_replaces_the_selected_image() {
        let (_dir, mut state) = loaded();
        state.select_image(Some(TEXTURE.into()));
        state.receive_files(vec![file("anything.png", my_png(4, 4))]);
        assert!(state.image_override(TEXTURE).is_some());
        assert!(state.images.import.is_none());
        assert_eq!(state.images.notice.as_ref().unwrap().tone, Tone::Good);
    }

    #[test]
    fn with_nothing_selected_one_file_is_matched_by_name() {
        let (_dir, mut state) = loaded();
        state.receive_files(vec![file("hero_ally.png", my_png(4, 4))]);
        let review = state.images.import.clone().unwrap();
        assert_eq!(review.plan.matched[0].game_path, TEXTURE);
        assert!(state.image_override(TEXTURE).is_none(), "waits for Import");
        state.receive_files(vec![file("nothing.png", my_png(4, 4))]);
        assert!(
            state
                .images
                .notice
                .as_ref()
                .unwrap()
                .text
                .starts_with("None of these files")
        );
    }

    #[test]
    fn a_zip_is_reviewed_then_imported_as_one_undo_step() {
        let (_dir, mut state) = loaded();
        state.select_image(Some(ITEM.into()));
        state.replace_image(ITEM, &my_png(2, 2));
        let when = chrono::NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let mut zip = ZipWriter::new(Vec::new(), when);
        zip.add("pack/minimap/hero_ally_psd.png", &my_png(8, 8))
            .unwrap();
        zip.add("pack/items/stand_in.png", &my_png(8, 8)).unwrap();
        zip.add("pack/hud/top_bar/icon_ultimate.svg", MY_SVG.as_bytes())
            .unwrap();
        zip.add("pack/readme.txt", b"hi").unwrap();
        zip.add("pack/unknown.png", &my_png(1, 1)).unwrap();
        let bytes = zip.finish().unwrap();
        state.receive_files(vec![file("pack.zip", bytes)]);
        let review = state.images.import.clone().unwrap();
        assert_eq!(review.title, "pack");
        assert_eq!(review.plan.matched.len(), 3);
        assert_eq!(review.replaces_changes, 1);
        assert_eq!(
            review.plan.unmatched().cloned().collect::<Vec<_>>(),
            [("pack/unknown.png".to_string(), Skip::NoMatch)]
        );
        assert!(state.image_override(ITEM).is_some());
        let undo_before = state.images.edit.history.undo.len();
        state.confirm_import();
        assert!(state.images.import.is_none());
        assert_eq!(state.images_changed_count(), 3);
        assert!(state.images.changed_only, "shows what came in");
        assert!(matches!(
            state.image_override(TOP_BAR).map(|o| &o.source),
            Some(Source::Svg { .. })
        ));
        assert!(
            state
                .images
                .notice
                .as_ref()
                .unwrap()
                .text
                .starts_with("Imported 3 images from pack")
        );
        assert_eq!(state.images.edit.history.undo.len(), undo_before + 1);
        state.undo_images();
        assert_eq!(state.images_changed_count(), 1);
    }

    #[test]
    fn a_folder_imports_and_bad_files_are_reported() {
        let (dir, mut state) = loaded();
        let pack = dir.path().join("My Icons");
        std::fs::create_dir_all(pack.join("minimap")).unwrap();
        std::fs::write(pack.join("minimap/hero_ally_psd.png"), my_png(4, 4)).unwrap();
        std::fs::write(pack.join("stand_in.png"), b"\x89PNG\r\n\x1a\nbroken").unwrap();
        state.receive_files(vec![Incoming::from_path(&pack)]);
        assert_eq!(state.images.import.as_ref().unwrap().title, "My Icons");
        state.confirm_import();
        assert_eq!(state.images_changed_count(), 1);
        let notice = state.images.notice.clone().unwrap();
        assert_eq!(notice.tone, Tone::Warn);
        assert!(
            notice.text.contains("1 couldn't be used; stand_in.png"),
            "{}",
            notice.text
        );
        state.receive_files(vec![file("broken.zip", b"PK\x03\x04nope".to_vec())]);
        assert!(
            state
                .images
                .notice
                .unwrap()
                .text
                .contains("couldn't be read as a zip")
        );
    }

    #[test]
    fn exporting_picked_images_saves_each_as_the_game_has_it() {
        let (_dir, mut state) = loaded();
        state.export_images(&[TEXTURE.to_string(), TOP_BAR.to_string()]);
        let out = state.exports_dir();
        assert!(out.join("minimap/hero_ally_psd.png").is_file());
        assert_eq!(
            std::fs::read_to_string(out.join("hud/top_bar/icon_ultimate.svg")).unwrap(),
            GAME_SVG
        );
        assert!(
            state
                .images
                .notice
                .unwrap()
                .text
                .starts_with("Saved 2 images")
        );
    }

    #[test]
    fn my_changes_export_finished_and_import_back() {
        let (_dir, mut state) = loaded();
        state.select_image(Some(TEXTURE.into()));
        state.replace_image(TEXTURE, &my_png(40, 20));
        state.set_image_fit(Fit::Fill);
        state.adjust_images(&[TOP_BAR.to_string()], Adjust::Invert);
        state.adjust_images(&[ITEM.to_string()], Adjust::Opacity { percent: 50 });
        let file = state.export_changes().unwrap();
        let files = zip::files(&std::fs::read(&file).unwrap()).unwrap();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "panorama/images/hud/top_bar/icon_ultimate.svg",
                "panorama/images/items/stand_in_psd.png",
                "panorama/images/minimap/hero_ally_psd.png",
            ]
        );
        let placed = png::read(&files[2].1).unwrap();
        assert_eq!(
            (placed.width, placed.height),
            (32, 16),
            "at the game's size"
        );
        let faded = png::read(&files[1].1).unwrap();
        assert_eq!(faded.pixel(0, 0)[3], 128);
        let svg_text = String::from_utf8(files[0].1.clone()).unwrap();
        assert_eq!(svg_text, svg::adjust(GAME_SVG, &[Adjust::Invert]));
        assert!(state.export_changes().unwrap() != file, "never overwrites");

        state.reset_all_images();
        state.receive_files(vec![Incoming::from_path(&file)]);
        state.confirm_import();
        assert_eq!(state.images_changed_count(), 3);
        assert!(matches!(
            state.image_override(TOP_BAR).map(|o| &o.source),
            Some(Source::Svg { .. })
        ));
    }

    #[test]
    fn a_dragged_position_saves_once_it_rests() {
        let (_dir, mut state) = loaded();
        state.select_image(Some(TEXTURE.into()));
        state.replace_image(TEXTURE, &my_png(40, 20));
        let window = Crop {
            x: 10,
            y: 0,
            width: 20,
            height: 10,
        };
        let t0 = std::time::Instant::now();
        state.drag_crop(TEXTURE, window, t0);
        assert_eq!(state.shown_crop(TEXTURE), Some(window));
        assert_eq!(
            state.image_override(TEXTURE).and_then(IconOverride::crop),
            None
        );
        assert!(state.settle_edits(t0));
        assert!(state.can_undo());
        assert!(!state.settle_edits(t0 + std::time::Duration::from_secs(1)));
        assert_eq!(
            state.image_override(TEXTURE).and_then(IconOverride::crop),
            Some(window)
        );
        let frame = state.image_frame(TEXTURE).unwrap();
        assert_eq!(frame.slot, (32, 16));
        state.set_image_fit(Fit::Original);
        assert_eq!(state.shown_crop(TEXTURE), None, "a fit snaps it back");
        state.undo_images();
        assert_eq!(state.shown_crop(TEXTURE), Some(window));
        state.drag_crop(ITEM, window, t0);
        assert!(
            state.images.crop_draft.is_none(),
            "only a replaced PNG moves"
        );
    }
}
