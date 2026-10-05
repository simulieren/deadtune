//! Local texture downscaling. Instead of downloading a prebuilt "low VRAM" texture pack,
//! DeadTune reads the game's own `.vtex_c` files, drops their largest mip levels
//! (`vtex`), and packs the reduced copies into one DeadTune-owned addon VPK that
//! overrides them (`addon`). The game files are never written; removing the addon
//! restores full quality.
//!
//! `decode` turns any UI texture into RGBA8 (`png` writes and reads PNG, `svg` handles the
//! compiled vector icons) for previews and snapshots.

pub mod addon;
pub mod decode;
pub mod png;
pub mod resample;
pub mod select;
pub mod svg;
pub mod vtex;

pub use addon::{AddonError, Progress, Stats, build_texture_addon};
pub use decode::RgbaImage;
pub use select::{Category, Factor, TextureDownscale};
pub use vtex::{SkipReason, Vtex, VtexError, downscale};
