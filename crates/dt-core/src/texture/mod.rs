//! Local texture downscaling. Instead of downloading a prebuilt "low VRAM" texture pack,
//! DeadTune reads the game's own `.vtex_c` files, drops their largest mip levels
//! (`vtex`), and packs the reduced copies into one DeadTune-owned addon VPK that
//! overrides them (`addon`). The game files are never written; removing the addon
//! restores full quality.
//!
//! UI images go both ways: `decode` turns any Panorama texture into RGBA8 for previews
//! and snapshots (`png` writes and reads PNG, `svg` handles the compiled vector icons),
//! `encode` writes a player's image back into the game's container.

pub mod addon;
pub mod adjust;
pub mod decode;
pub mod encode;
pub mod frame;
pub mod png;
pub mod resample;
pub mod select;
pub mod svg;
pub mod vtex;

pub use addon::{AddonError, Progress, Stats, build_texture_addon};
pub use decode::{DecodeError, decode, decode_mip, thumbnail};
pub use png::RgbaImage;
pub use select::{Category, Factor, TextureDownscale};
pub use vtex::{SkipReason, Vtex, VtexError, downscale};
