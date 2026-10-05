//! The game's own images for the HUD previews (map, frame, markers, portraits, top bar and
//! health bar parts, `dt_core::hud::art`), decoded off the UI thread by `crate::thumbs`.
//! A preview asks for each picture by game path and draws its shapes while the picture is
//! missing, still decoding or unreadable.

use std::path::PathBuf;
use std::sync::Arc;

use dt_core::hud::art::Art;
use dt_core::snapshot::store::TEXT;
use eframe::egui::epaint::{Mesh, Vertex};
use eframe::egui::{self, Color32, Painter, Pos2, Rect, TextureHandle, pos2, vec2};

use crate::images::{ImageSource, Picture};
use crate::state::AppState;
use crate::thumbs::{Slot, Thumbs};

#[derive(Default)]
pub struct HudArtState {
    /// `DEADTUNE_PREVIEW_IMAGES`: wins over `settings.preview_images` for this run.
    pub from: Option<PathBuf>,
    /// `DEADTUNE_PREVIEW_SHAPES=1`: no source, so every preview draws its shapes.
    pub shapes_only: bool,
    /// Made when a preview is first drawn.
    pub thumbs: Option<Thumbs>,
}

impl HudArtState {
    pub fn busy(&self) -> bool {
        self.thumbs.as_ref().is_some_and(Thumbs::busy)
    }
}

impl AppState {
    /// Where the previews read game images, asked in this order: a decoded-image folder the
    /// player chose (it is there on purpose, to compare against), the game's pak01, then the
    /// snapshot the UI images page previews (`DEADTUNE_IMAGES_FROM`).
    pub fn preview_source(&self) -> ImageSource {
        let mut chain = Vec::new();
        if self.hud_art.shapes_only {
            return ImageSource::Chain(chain);
        }
        if let Some(dir) = self
            .hud_art
            .from
            .as_ref()
            .or(self.settings.preview_images.as_ref())
        {
            chain.push(ImageSource::decoded(dir));
        }
        if let Ok(game) = ImageSource::game(&self.paths) {
            chain.push(game);
        }
        if let Some(dir) = &self.images.from {
            chain.push(ImageSource::folder(dir));
            chain.push(ImageSource::decoded(&dir.join(TEXT)));
        }
        ImageSource::Chain(chain)
    }

    /// The picture a preview shows for `game_path`: its override from `profile.hud.icons`
    /// (the player's file, the game's image, or either with the colour edits on it, the
    /// sliders' draft first), else the game's own. Every preview and the UI images page go
    /// through here, so an edit shows in all of them; `icons::build` applies the same list.
    pub fn preview_picture(&self, game_path: &str, side: u32) -> Picture {
        let file = self.stored_image(game_path);
        let adjust = self.shown_adjustments(game_path);
        match (file, adjust.is_empty()) {
            (Some(file), true) => Picture::Mine { file, side },
            (None, true) => Picture::Game {
                path: game_path.to_string(),
                side,
            },
            (file, false) => Picture::Edited {
                path: game_path.to_string(),
                file,
                adjust: adjust.to_vec(),
                side,
            },
        }
    }

    /// `game_path` as the previews draw it, decoded now.
    #[cfg(test)]
    pub fn preview_image(
        &self,
        source: &ImageSource,
        game_path: &str,
        side: u32,
    ) -> Result<dt_core::texture::RgbaImage, String> {
        crate::images::render(source, &self.preview_picture(game_path, side)).map(|r| r.image)
    }
}

/// The pictures one frame of a page asks for.
pub struct Images<'a> {
    thumbs: &'a mut Thumbs,
    state: &'a AppState,
    pixels_per_point: f32,
}

impl Images<'_> {
    /// `art` decoded for drawing `points` wide (its longer side), if it is ready.
    pub fn get(&mut self, art: Art, points: f32) -> Option<TextureHandle> {
        let px = (points * self.pixels_per_point).max(16.0) as u32;
        let side = px.next_power_of_two().min(1024);
        match self
            .thumbs
            .get(&self.state.preview_picture(art.path, side))?
        {
            Slot::Ready { texture, .. } => Some(texture),
            Slot::Failed(_) => None,
        }
    }

    /// Paints `art` into `rect`, washed with `tint`; false (and nothing drawn) when the
    /// picture is not available, so the caller draws its stand-in shape instead.
    pub fn paint(&mut self, p: &Painter, art: Art, rect: Rect, tint: Color32) -> bool {
        match self.get(art, rect.width().max(rect.height())) {
            Some(texture) => {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                p.image(texture.id(), rect, uv, tint);
                true
            }
            None => false,
        }
    }

    /// Paints the top of `art` cut to a circle of `radius` around `centre`, as Panorama's
    /// round portrait masks do; false when the picture is not available.
    pub fn paint_disc(
        &mut self,
        p: &Painter,
        art: Art,
        centre: Pos2,
        radius: f32,
        tint: Color32,
    ) -> bool {
        let Some(texture) = self.get(art, radius * 2.0) else {
            return false;
        };
        let [w, h] = art.size.map(f32::from);
        let (du, dv) = if w <= h {
            (0.5, 0.5 * w / h)
        } else {
            (0.5 * h / w, 0.5)
        };
        let middle = pos2(0.5, dv);
        let mut mesh = Mesh::with_texture(texture.id());
        mesh.vertices.push(Vertex {
            pos: centre,
            uv: middle,
            color: tint,
        });
        let n = 48u32;
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let (sin, cos) = a.sin_cos();
            mesh.vertices.push(Vertex {
                pos: centre + vec2(cos, sin) * radius,
                uv: middle + vec2(cos * du, sin * dv),
                color: tint,
            });
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
        }
        p.add(mesh);
        true
    }
}

/// Runs `draw` with the previews' pictures, made on first use.
pub fn with<R>(
    ctx: &egui::Context,
    state: &mut AppState,
    draw: impl FnOnce(&AppState, &mut Images) -> R,
) -> R {
    let mut thumbs = state
        .hud_art
        .thumbs
        .take()
        .unwrap_or_else(|| Thumbs::new(ctx, Arc::new(state.preview_source())));
    thumbs.poll();
    let out = {
        let state = &*state;
        let mut images = Images {
            thumbs: &mut thumbs,
            state,
            pixels_per_point: ctx.pixels_per_point(),
        };
        draw(state, &mut images)
    };
    thumbs.end_frame();
    state.hud_art.thumbs = Some(thumbs);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::images::tests::{ITEM, TEXTURE, TOP_BAR, install_images, my_png};
    use crate::state::testutil;
    use dt_core::texture::adjust::{Adjust, Rgb};
    use dt_core::texture::{RgbaImage, png};

    fn write_png(file: &std::path::Path, rgba: [u8; 4]) {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let image = RgbaImage::new(4, 4, rgba.repeat(16)).unwrap();
        std::fs::write(file, png::write(&image).unwrap()).unwrap();
    }

    const ALLY_PNG: &str = "panorama/images/minimap/hero_ally_psd.png";

    #[test]
    fn a_chosen_image_folder_wins_then_the_game_then_the_snapshot() {
        let (dir, mut state) = testutil::state();
        install_images(&state);
        let source = state.preview_source();
        assert_eq!(
            state
                .preview_image(&source, TEXTURE, 8)
                .unwrap()
                .pixel(0, 0),
            [0, 0, 255, 255],
            "the game's pak"
        );

        let export = dir.path().join("export");
        write_png(&export.join(ALLY_PNG), [1, 2, 3, 255]);
        state.settings.preview_images = Some(export);
        let source = state.preview_source();
        assert_eq!(
            state
                .preview_image(&source, TEXTURE, 8)
                .unwrap()
                .pixel(0, 0),
            [1, 2, 3, 255]
        );
        assert_eq!(
            state.preview_image(&source, ITEM, 8).unwrap().pixel(0, 0),
            [0, 255, 0, 255],
            "what the folder lacks still comes from the game"
        );

        let one_run = dir.path().join("one_run");
        write_png(&one_run.join(ALLY_PNG), [7, 7, 7, 255]);
        state.hud_art.from = Some(one_run);
        let source = state.preview_source();
        assert_eq!(
            state
                .preview_image(&source, TEXTURE, 8)
                .unwrap()
                .pixel(0, 0),
            [7, 7, 7, 255]
        );

        let snapshot = dir.path().join("snap");
        write_png(
            &snapshot.join("text/panorama/images/hud/only_here_psd.png"),
            [5, 5, 5, 255],
        );
        state.images.from = Some(snapshot);
        let source = state.preview_source();
        assert_eq!(
            state
                .preview_image(&source, "panorama/images/hud/only_here_psd.vtex_c", 8)
                .unwrap()
                .pixel(0, 0),
            [5, 5, 5, 255],
            "a snapshot's decoded pictures fill in last"
        );
    }

    #[test]
    fn previews_show_the_players_replacement() {
        let (_dir, mut state) = testutil::state();
        install_images(&state);
        let source = state.preview_source();
        state.replace_image(TEXTURE, &my_png(8, 4));
        let shown = state.preview_image(&source, TEXTURE, 8).unwrap();
        assert_eq!(shown.pixel(0, 0), [255, 0, 0, 255]);
        assert!(matches!(
            state.preview_picture(TEXTURE, 8),
            Picture::Mine { .. }
        ));
        state.reset_image(TEXTURE);
        assert_eq!(
            state
                .preview_image(&source, TEXTURE, 8)
                .unwrap()
                .pixel(0, 0),
            [0, 0, 255, 255]
        );
    }

    #[test]
    fn previews_show_colour_edits() {
        let (_dir, mut state) = testutil::state();
        install_images(&state);
        let source = state.preview_source();
        let navy = Adjust::Tint {
            color: Rgb([0, 0, 128]),
            strength: 100,
        };
        state.adjust_images(&[TEXTURE.to_string()], navy);
        assert!(matches!(
            state.preview_picture(TEXTURE, 8),
            Picture::Edited { file: None, .. }
        ));
        assert_eq!(
            state
                .preview_image(&source, TEXTURE, 8)
                .unwrap()
                .pixel(0, 0),
            [0, 0, 128, 255],
            "blue multiplied by navy"
        );

        let white = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 8\"><rect width=\"8\" height=\"8\" fill=\"#ffffff\"/></svg>";
        state.replace_image(TOP_BAR, white.as_bytes());
        let swap = Adjust::Swap {
            from: Rgb::WHITE,
            to: Rgb([255, 0, 0]),
        };
        state.adjust_images(&[TOP_BAR.to_string()], swap);
        assert_eq!(
            state
                .preview_image(&source, TOP_BAR, 8)
                .unwrap()
                .pixel(4, 4),
            [255, 0, 0, 255],
            "the swap shows on the player's SVG"
        );
    }
}
