//! Local texture downscaling. Instead of downloading a prebuilt "low VRAM" texture pack,
//! DeadTune reads the game's own `.vtex_c` files, drops their largest mip levels
//! (`vtex`), and packs the reduced copies into one DeadTune-owned addon VPK that
//! overrides them (`addon`). The game files are never written; removing the addon
//! restores full quality.

pub mod vtex;

pub use vtex::{SkipReason, Vtex, VtexError, downscale};
