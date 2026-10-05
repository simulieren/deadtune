//! Writes a `pak01_dir.vpk` carrying the game files the native addon builders read, so a
//! fake install (`scripts/fake-install.sh`) can build every addon: the stylesheet as
//! Sqooky's pak97 copied it, the game's empty particle, the Sinner's Sacrifice mask and
//! model in their stock layout (made from the upstream pak like the unit tests do), a
//! 512 px stand-in for the scope overlay, and the top bar's and minimap's stylesheets and
//! layouts, a stand-in settings menu with the rows our in-game settings anchor on, and the
//! HUD and health stylesheets (the HUD stylesheet standing in for every stylesheet).
//!
//! cargo run -p dt-core --example fake_pak01 -- <out pak01_dir.vpk>

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use dt_core::addons::{native_blur, native_particles, native_scope, native_sinner};
use dt_core::hud::apples_tunnels::MINIMAP_LAYOUT;
use dt_core::hud::elements::HUD_STYLE;
use dt_core::hud::health_style::{HEALTH_CONTAINER_STYLE, HEALTH_STYLE};
use dt_core::hud::ingame::{self, SETTINGS_LAYOUT};
use dt_core::hud::inject;
use dt_core::hud::minimap_colors::MINIMAP_STYLE;
use dt_core::hud::resource::Resource;
use dt_core::hud::topbar::{TOP_BAR_LAYOUT, TOP_BAR_STYLE};
use dt_core::hud::vpk::{self, VpkDir};
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
            scope_original(&pak89, 512)?,
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
    for style in [HUD_STYLE, HEALTH_STYLE, HEALTH_CONTAINER_STYLE] {
        files.insert(
            style.to_string(),
            std::fs::read(here("tests/fixtures/hud/hud_vanilla.vcss_c"))?,
        );
    }
    std::fs::write(&out, vpk::write(&files))?;
    eprintln!("wrote {out} with {} entries", files.len());
    Ok(())
}
