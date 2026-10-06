//! Writes a `pak01_dir.vpk` carrying the game files the native addon builders read, so a
//! fake install (`scripts/fake-install.sh`) can build every addon: the stylesheet as
//! Sqooky's pak97 copied it, the game's empty particle, the Sinner's Sacrifice mask and
//! model in their stock layout (made from the upstream pak like the unit tests do), a
//! 2048 px stand-in for the scope overlay (the game's size), and the top bar's and minimap's
//! stylesheets and layouts, a stand-in settings menu with the rows our in-game settings anchor
//! on, the HUD and health stylesheets (the HUD stylesheet standing in for every stylesheet),
//! and a vector icon for UI image overrides (a Panorama container laid out as DeadTune reads
//! it, not a copy of the game's file), plus a few dozen generated stand-in images (shapes and
//! gradients in the game's folders, and one undecodable file) for the UI images page, and one
//! at every path the HUD previews draw (`hud::art`).
//!
//! cargo run -p dt-core --example fake_pak01 -- <out pak01_dir.vpk>

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use dt_core::addons::{native_blur, native_particles, native_scope, native_sinner};
use dt_core::hud::apples_tunnels::MINIMAP_LAYOUT;
use dt_core::hud::art;
use dt_core::hud::crc32::crc32;
use dt_core::hud::elements::HUD_STYLE;
use dt_core::hud::health_style::{HEALTH_CONTAINER_STYLE, HEALTH_STYLE};
use dt_core::hud::ingame::{self, SETTINGS_LAYOUT};
use dt_core::hud::inject;
use dt_core::hud::minimap_colors::MINIMAP_STYLE;
use dt_core::hud::resource::{Block, Resource};
use dt_core::hud::topbar::{TOP_BAR_LAYOUT, TOP_BAR_STYLE};
use dt_core::hud::vpk::{self, VpkDir};
use dt_core::texture::RgbaImage;
use dt_core::texture::encode::{self, Fit};
use dt_core::texture::vtex::Vtex;

const UPSTREAM: &str =
    "../../research/configs/OptimizationLock/Various Addons Relating to Performance";

fn here(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn data_block(res: &Resource) -> &[u8] {
    &res.block(b"DATA").expect("DATA block").data
}

/// The upstream mask with the ten smaller ATI1N levels put back and `NO_LOD` cleared.
fn stock_mask(upstream: &VpkDir) -> Result<Vec<u8>, Box<dyn Error>> {
    let up = upstream.read(native_sinner::MASK)?;
    let v = Vtex::parse(&up)?;
    let header = v.pixel_start() - data_block(&Resource::parse(&up)?).len();
    let mut out = up[..v.pixel_start()].to_vec();
    out[header + 2] &= !0x08;
    out[header + 27] = 11;
    for level in (1..11u8).rev() {
        let blocks = (1024usize >> level).max(1).div_ceil(4);
        out.extend(std::iter::repeat_n(level, blocks * blocks * 8));
    }
    out.extend_from_slice(&up[v.pixel_start()..]);
    Ok(out)
}

/// The upstream model with the stock (LZ4, unpatched) DATA block from the test fixture.
fn stock_model(upstream: &VpkDir) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut res = Resource::parse(&upstream.read(native_sinner::MODEL)?)?;
    let block = res
        .blocks
        .iter_mut()
        .find(|b| &b.name == b"DATA")
        .expect("DATA block");
    block.data = std::fs::read(here("tests/fixtures/sinner/stock_model_data.kv3"))?;
    Ok(res.to_bytes())
}

/// Upstream's 1080 px scope texture resampled (nearest) to `side`, header dims patched.
fn scope_original(upstream: &VpkDir, side: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let up = upstream.read(native_scope::TEXTURE)?;
    let v = Vtex::parse(&up)?;
    let res = Resource::parse(&up)?;
    let width_at = v.pixel_start() - data_block(&res).len() + 20;
    let (w, start) = (v.width as usize, v.pixel_start());
    let mut out = up[..start].to_vec();
    out[width_at..width_at + 2].copy_from_slice(&(side as u16).to_le_bytes());
    out[width_at + 2..width_at + 4].copy_from_slice(&(side as u16).to_le_bytes());
    for y in 0..side {
        for x in 0..side {
            let i = start + ((y * w / side) * w + x * w / side) * 4;
            out.extend_from_slice(&up[i..i + 4]);
        }
    }
    Ok(out)
}

const VECTOR_ICON: &str = "panorama/images/hud/icons/rejuvenator.vsvg_c";
const VECTOR_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><circle cx=\"16\" cy=\"16\" r=\"12\"/></svg>";

/// `svg` in a Panorama resource: RED2, then DATA with a CRC prefix, an empty image table
/// and the text.
fn vector_icon(svg: &str) -> Vec<u8> {
    let mut data = crc32(svg.as_bytes()).to_le_bytes().to_vec();
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(svg.as_bytes());
    Resource {
        header_version: 12,
        type_version: 3,
        blocks: vec![
            Block {
                name: *b"RED2",
                data: vec![0; 16],
            },
            Block {
                name: *b"DATA",
                data,
            },
        ],
    }
    .to_bytes()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Ring,
    Disc,
    Diamond,
    Square,
    Bar,
    Portrait,
    Map,
    Frame,
}

/// A generated stand-in picture: `shape` in `rgb` on transparency, softly edged.
fn picture(width: u32, height: u32, shape: Shape, rgb: [u8; 3]) -> RgbaImage {
    let (w, h) = (width as f32, height as f32);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let u = (x as f32 + 0.5) / w * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / h * 2.0 - 1.0;
            let r = (u * u + v * v).sqrt();
            let edge = |d: f32| (d * w.min(h) * 0.5).clamp(0.0, 1.0);
            let (alpha, shade) = match shape {
                Shape::Ring => (edge(0.18 - (r - 0.68).abs()), 1.0),
                Shape::Disc => (edge(0.8 - r), 1.0 - 0.35 * r),
                Shape::Diamond => (edge(0.85 - u.abs() - v.abs()), 1.0 - 0.3 * v),
                Shape::Square => (edge(0.8 - u.abs().max(v.abs())), 0.75 - 0.25 * v),
                Shape::Bar => (edge(0.7 - v.abs()), 0.45 + 0.55 * (u + 1.0) / 2.0),
                Shape::Portrait => {
                    let head = (u * u + (v + 0.25).powi(2)).sqrt();
                    let body = (u * u * 0.6 + (v - 0.95).powi(2)).sqrt();
                    let figure = head < 0.38 || body < 0.62;
                    (1.0, if figure { 1.0 } else { 0.35 + 0.2 * (1.0 - v) })
                }
                Shape::Frame => {
                    let notch = v < -0.9 && u.abs() < 0.08;
                    (
                        edge(0.05 - (r - 0.93).abs()).max(if notch { 1.0 } else { 0.0 }),
                        0.8 + 0.2 * v,
                    )
                }
                Shape::Map => {
                    let lane = [-0.62f32, -0.2, 0.2, 0.62]
                        .iter()
                        .any(|x| (u - x * (1.0 - 0.25 * v.abs())).abs() < 0.035);
                    let mid = (r - 0.16).abs() < 0.03 || v.abs() < 0.02 && u.abs() < 0.9;
                    (
                        edge(0.97 - r),
                        if lane || mid { 1.0 } else { 0.42 + 0.1 * u * v },
                    )
                }
            };
            let c = |ch: u8| (f32::from(ch) * shade).min(255.0) as u8;
            pixels.extend_from_slice(&[c(rgb[0]), c(rgb[1]), c(rgb[2]), (alpha * 255.0) as u8]);
        }
    }
    RgbaImage::new(width, height, pixels).expect("sized")
}

const STAND_IN_SVGS: [(&str, &str); 12] = [
    (
        "minimap/ping_danger",
        r##"<path d="M16 3 L30 28 H2 Z" fill="#ef6b6b"/><rect x="15" y="11" width="2" height="9" fill="#fff"/>"##,
    ),
    (
        "minimap/ping_go",
        r##"<path d="M4 16 H22 M15 8 L24 16 L15 24" stroke="#5fcb8c" stroke-width="4" fill="none"/>"##,
    ),
    (
        "hud/top_bar/icon_ultimate",
        r##"<path d="M16 2 L20 12 L31 12 L22 19 L25 30 L16 23 L7 30 L10 19 L1 12 L12 12 Z" fill="#f0b341"/>"##,
    ),
    (
        "hud/top_bar/icon_dead",
        r##"<circle cx="16" cy="16" r="13" fill="#2b2f37"/><path d="M10 10 L22 22 M22 10 L10 22" stroke="#ef6b6b" stroke-width="4"/>"##,
    ),
    (
        "hud/top_bar/soul_orb",
        r##"<circle cx="16" cy="16" r="12" fill="#56b4e9"/><circle cx="12" cy="12" r="4" fill="#d6f0ff"/>"##,
    ),
    (
        "upgrades/stand_in_weapon",
        r##"<rect x="4" y="4" width="24" height="24" rx="5" fill="#e69f00"/><path d="M9 23 L23 9" stroke="#fff" stroke-width="3"/>"##,
    ),
    (
        "upgrades/stand_in_vitality",
        r##"<rect x="4" y="4" width="24" height="24" rx="5" fill="#5fcb8c"/><path d="M16 9 V23 M9 16 H23" stroke="#fff" stroke-width="3"/>"##,
    ),
    (
        "upgrades/stand_in_spirit",
        r##"<rect x="4" y="4" width="24" height="24" rx="5" fill="#b07cf0"/><circle cx="16" cy="16" r="6" fill="none" stroke="#fff" stroke-width="3"/>"##,
    ),
    (
        "icons/stand_in_lock",
        r##"<rect x="7" y="14" width="18" height="14" rx="2" fill="#ece8e1"/><path d="M11 14 V10 A5 5 0 0 1 21 10 V14" stroke="#ece8e1" stroke-width="3" fill="none"/>"##,
    ),
    (
        "icons/stand_in_gear",
        r##"<circle cx="16" cy="16" r="9" fill="none" stroke="#ece8e1" stroke-width="5" stroke-dasharray="4 3"/><circle cx="16" cy="16" r="4" fill="#ece8e1"/>"##,
    ),
    (
        "icons/stand_in_bell",
        r##"<path d="M8 22 Q8 8 16 7 Q24 8 24 22 Z" fill="#ece8e1"/><circle cx="16" cy="25" r="2.5" fill="#ece8e1"/>"##,
    ),
    (
        "icons/stand_in_eye",
        r##"<path d="M2 16 Q16 4 30 16 Q16 28 2 16 Z" fill="#ece8e1"/><circle cx="16" cy="16" r="5" fill="#15171b"/>"##,
    ),
];

/// Generated stand-ins under `panorama/images/`, so the UI images page has something to
/// show on a machine without the game. Textures reuse `template`'s container.
fn stand_in_images(template: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn Error>> {
    use Shape::*;
    let textures: [(&str, u32, u32, Shape, [u8; 3]); 20] = [
        ("minimap/hero_ally", 64, 64, Ring, [86, 180, 233]),
        ("minimap/hero_enemy", 64, 64, Ring, [230, 159, 0]),
        (
            "minimap/objective_guardian",
            64,
            64,
            Diamond,
            [236, 232, 225],
        ),
        ("minimap/objective_walker", 96, 96, Diamond, [240, 179, 65]),
        ("minimap/urn", 48, 48, Square, [95, 203, 140]),
        ("minimap/soul_orb", 32, 32, Disc, [86, 180, 233]),
        ("hud/top_bar/soul_lead_bar", 256, 32, Bar, [240, 179, 65]),
        ("hud/top_bar/clock_bg", 128, 64, Square, [70, 76, 88]),
        ("hud/health_bar_fill", 256, 24, Bar, [95, 203, 140]),
        ("hud/crosshair/dot", 32, 32, Disc, [255, 255, 255]),
        (
            "heroes/stand_in_01_card",
            96,
            128,
            Portrait,
            [176, 124, 240],
        ),
        ("heroes/stand_in_02_card", 96, 128, Portrait, [230, 159, 0]),
        ("heroes/stand_in_03_card", 96, 128, Portrait, [86, 180, 233]),
        (
            "heroes/stand_in_04_card",
            96,
            128,
            Portrait,
            [239, 107, 107],
        ),
        ("heroes/stand_in_01_mm", 64, 64, Disc, [176, 124, 240]),
        ("heroes/stand_in_02_mm", 64, 64, Disc, [230, 159, 0]),
        ("items/stand_in_shield", 64, 64, Square, [86, 180, 233]),
        ("items/stand_in_blade", 64, 64, Diamond, [230, 159, 0]),
        ("items/stand_in_orb", 64, 64, Disc, [176, 124, 240]),
        ("items/stand_in_ring", 64, 64, Ring, [95, 203, 140]),
    ];
    let mut out = BTreeMap::new();
    for (name, w, h, shape, rgb) in textures {
        out.insert(
            format!("panorama/images/{name}_psd.vtex_c"),
            encode::replace(template, &picture(w, h, shape, rgb), Fit::Own)?,
        );
    }
    for (name, body) in STAND_IN_SVGS {
        let svg =
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">{body}</svg>"#);
        out.insert(format!("panorama/images/{name}.vsvg_c"), vector_icon(&svg));
    }
    out.insert(
        "panorama/images/hud/stand_in_unreadable_psd.vtex_c".into(),
        b"not a texture".to_vec(),
    );
    Ok(out)
}

/// A stand-in at every path the HUD previews draw (`hud::art`), sized like the game's CSS
/// box and coloured from the path, so the previews show their image mode without the game.
fn preview_stand_ins(template: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn Error>> {
    let mut out = BTreeMap::new();
    for a in art::all() {
        let seed = dt_core::hud::crc32::crc32(a.path.as_bytes()).to_le_bytes();
        let rgb = [seed[0] | 0x50, seed[1] | 0x50, seed[2] | 0x50];
        if a.path.ends_with(".vsvg_c") {
            let [w, h] = a.size;
            let fill = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            let (wf, hf) = (f32::from(w), f32::from(h));
            let short = wf.min(hf);
            let frame = a.path.contains("healthbar") || a.path.contains("backer");
            let dot = if frame {
                String::new()
            } else {
                format!(
                    r#"<circle cx="{}" cy="{}" r="{}" fill="{fill}"/>"#,
                    wf / 2.0,
                    hf / 2.0,
                    short * 0.15
                )
            };
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"><rect x="{x}" y="{y}" width="{iw}" height="{ih}" rx="{r}" fill="none" stroke="{fill}" stroke-width="{sw}"/>{dot}</svg>"##,
                x = wf * 0.1,
                y = hf * 0.1,
                iw = wf * 0.8,
                ih = hf * 0.8,
                r = short * 0.25,
                sw = short * 0.1,
            );
            out.insert(a.path.to_string(), vector_icon(&svg));
            continue;
        }
        let shape = if a.path.contains("/minimap/base/") {
            Shape::Map
        } else if a.path.contains("_card_") {
            Shape::Portrait
        } else if a.path.contains("compass_frame") {
            Shape::Frame
        } else if a.path.contains("boss_health") {
            Shape::Ring
        } else if a.path.contains("_mm_") || a.path.contains("marker") {
            Shape::Disc
        } else if a.path.contains("healthbar") {
            Shape::Square
        } else {
            Shape::Diamond
        };
        let rgb = match shape {
            Shape::Map => [96, 118, 104],
            Shape::Frame => [150, 156, 170],
            Shape::Ring => [236, 232, 225],
            _ => rgb,
        };
        let [w, h] = a.size.map(u32::from);
        let big = matches!(shape, Shape::Map | Shape::Frame);
        let k = (if big { 512 } else { 64 }) as f32 / w.max(h) as f32;
        let k = k.max(1.0);
        let (w, h) = ((w as f32 * k) as u32, (h as f32 * k) as u32);
        out.insert(
            a.path.to_string(),
            encode::replace(template, &picture(w, h, shape, rgb), Fit::Own)?,
        );
    }
    Ok(out)
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: fake_pak01 <out pak01_dir.vpk>")?;
    let pak97 = VpkDir::open(&here("tests/fixtures/hud/blur_pak97_dir.vpk"))?;
    let pak26 = VpkDir::open(&here(&format!(
        "{UPSTREAM}/Sinner Light Fix Mod/pak26_dir.vpk"
    )))?;
    let pak89 = VpkDir::open(&here(&format!(
        "{UPSTREAM}/Vindicta Scope Downscale/pak89_dir.vpk"
    )))?;
    let files = BTreeMap::from([
        (
            native_blur::STYLE.to_string(),
            pak97.read(native_blur::BASE)?,
        ),
        (
            native_particles::EMPTY_PARTICLE.to_string(),
            std::fs::read(here("tests/fixtures/particles/empty.vpcf_c"))?,
        ),
        (native_sinner::MASK.to_string(), stock_mask(&pak26)?),
        (native_sinner::MODEL.to_string(), stock_model(&pak26)?),
        (
            native_scope::TEXTURE.to_string(),
            scope_original(&pak89, 2048)?,
        ),
        (
            TOP_BAR_STYLE.to_string(),
            std::fs::read(here("tests/fixtures/hud/hud_vanilla.vcss_c"))?,
        ),
        (
            TOP_BAR_LAYOUT.to_string(),
            std::fs::read(here("tests/fixtures/hud/top_bar_vanilla.vxml_c"))?,
        ),
        (
            MINIMAP_STYLE.to_string(),
            std::fs::read(here("tests/fixtures/hud/hud_vanilla.vcss_c"))?,
        ),
        (
            MINIMAP_LAYOUT.to_string(),
            std::fs::read(here("tests/fixtures/hud/hud_minimap_vanilla.vxml_c"))?,
        ),
        (
            SETTINGS_LAYOUT.to_string(),
            inject::compiled_layout(&ingame::stand_in_layout()),
        ),
    ]);
    let mut files = files;
    files.extend(stand_in_images(&pak89.read(native_scope::TEXTURE)?)?);
    files.extend(preview_stand_ins(&pak89.read(native_scope::TEXTURE)?)?);
    files.insert(VECTOR_ICON.to_string(), vector_icon(VECTOR_SVG));
    let stats = dt_core::hud::player_stats::STYLES;
    for style in [HUD_STYLE, HEALTH_STYLE, HEALTH_CONTAINER_STYLE]
        .into_iter()
        .chain(stats)
    {
        files.insert(
            style.to_string(),
            std::fs::read(here("tests/fixtures/hud/hud_vanilla.vcss_c"))?,
        );
    }
    std::fs::write(&out, vpk::write(&files))?;
    eprintln!("wrote {out} with {} entries", files.len());
    Ok(())
}
