//! Simon's own in-game capture at default HUD settings (`assets/README.md`): the backdrop of
//! the HUD layout preview and the "In game" crops beside the minimap, top bar and health bar
//! previews. Element positions in it are `dt_core::hud::layout::reference_crop`.

use std::time::Instant;

use dt_core::hud::elements::ElementId;
use dt_core::hud::layout;
use eframe::egui::{
    self, Color32, ColorImage, Context, CornerRadius, Id, Painter, Rect, RichText, Sense, Stroke,
    StrokeKind, TextureHandle, Ui, Vec2, pos2, vec2,
};

use crate::theme;

static SHOT: &[u8] = include_bytes!("../assets/vanilla_hud.jpg");

fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let pixels = decoder.decode().map_err(|e| e.to_string())?;
    let (w, h) = decoder
        .dimensions()
        .ok_or_else(|| "no image size".to_string())?;
    Ok(ColorImage::from_rgba_unmultiplied([w, h], &pixels))
}

#[derive(Clone)]
struct Shot {
    texture: Option<TextureHandle>,
    drawn: Instant,
}

fn shot_id() -> Id {
    Id::new("game_shot")
}

/// The screenshot as a texture, decoded on first use and kept until [`release`].
pub fn texture(ctx: &Context) -> Option<TextureHandle> {
    let now = Instant::now();
    let kept = ctx.data_mut(|d| {
        let shot = d.get_temp_mut_or_insert_with(shot_id(), || None::<Shot>);
        shot.as_mut().map(|shot| {
            shot.drawn = now;
            shot.texture.clone()
        })
    });
    if let Some(texture) = kept {
        return texture;
    }
    let texture = decode(SHOT)
        .ok()
        .map(|image| ctx.load_texture("game_shot", image, egui::TextureOptions::LINEAR));
    let shot = Shot {
        texture: texture.clone(),
        drawn: now,
    };
    ctx.data_mut(|d| d.insert_temp(shot_id(), Some(shot)));
    texture
}

/// Frees the decoded screenshot (1280x720, 3.5 MB) when it was last drawn before `cutoff`;
/// otherwise says when it was last drawn.
pub fn release(ctx: &Context, cutoff: Instant) -> Option<Instant> {
    ctx.data_mut(|d| {
        let drawn = d.get_temp::<Option<Shot>>(shot_id()).flatten()?.drawn;
        if drawn < cutoff {
            d.remove::<Option<Shot>>(shot_id());
            return None;
        }
        Some(drawn)
    })
}

/// Paints the part of the screenshot at `crop` (x, y, width, height fractions) into `rect`.
pub fn paint(p: &Painter, texture: &TextureHandle, crop: [f32; 4], rect: Rect, tint: Color32) {
    let [x, y, w, h] = crop;
    let uv = Rect::from_min_max(pos2(x, y), pos2(x + w, y + h));
    p.image(texture.id(), rect, uv, tint);
}

/// A dimmed patch of the screenshot (the wooden door right of the crosshair, clear of any
/// HUD) behind a preview, so dark outlines read the way they do over the game.
pub fn backdrop(ui: &Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(8), theme::RAIL);
    let Some(texture) = texture(ui.ctx()) else {
        return;
    };
    let h = 0.38;
    let w = (rect.aspect_ratio() * h * 720.0 / 1280.0).min(0.3);
    let h = h.min(w * 1280.0 / 720.0 / rect.aspect_ratio());
    paint(
        &painter,
        &texture,
        [0.70 - w / 2.0, 0.22, w, h],
        rect,
        Color32::from_gray(105),
    );
}

/// `id` cut out of the screenshot at most `max` big, under a caption, so a preview can be
/// compared with the game at default settings. Nothing for elements the screenshot lacks.
pub fn in_game(ui: &mut Ui, id: ElementId, max: Vec2) {
    let (Some(texture), Some(crop)) = (texture(ui.ctx()), layout::reference_crop(id)) else {
        return;
    };
    let size = vec2(crop[2] * 16.0, crop[3] * 9.0);
    let k = (max.x / size.x).min(max.y / size.y);
    ui.label(
        RichText::new("In game, default HUD")
            .small()
            .color(theme::WEAK),
    );
    let (rect, _) = ui.allocate_exact_size(size * k, Sense::hover());
    paint(ui.painter(), &texture, crop, rect, Color32::WHITE);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Outside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_screenshot_decodes_at_16_by_9() {
        let image = decode(SHOT).expect("decodes");
        assert_eq!(image.size, [1280, 720]);
    }

    #[test]
    fn release_frees_only_a_screenshot_drawn_before_the_cutoff() {
        let ctx = Context::default();
        let before = Instant::now();
        let first = texture(&ctx).expect("decodes").id();
        let drawn = release(&ctx, before).expect("drawn after the cutoff, so kept");
        assert_eq!(texture(&ctx).expect("kept").id(), first);
        assert_eq!(release(&ctx, Instant::now() + std::time::Duration::from_nanos(1)), None);
        assert_ne!(texture(&ctx).expect("decoded again").id(), first);
        assert!(drawn >= before);
    }
}
