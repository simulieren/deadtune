//! HUD editing. Two levers, both outside the game process:
//! - HUD ConVars (crosshair, minimap icons, overhead bars) flow through the normal
//!   profile/apply pipeline; `convars` only supplies their catalog entries.
//! - Layout edits (move, scale, hide, fade) compile to CSS that is appended to the
//!   game's own compiled `hud.vcss_c` and shipped as one DeadTune-owned addon VPK.
//!
//! Pipeline: `HudLayout` -> `layout::compile` -> `StylePatch` -> `install::plan`
//! (reads the game's pak01, patches each style file via `resource`, packs via `vpk`)
//! -> `install::execute`.

pub mod convars;
pub mod crc32;
pub mod css;
pub mod elements;
pub mod install;
pub mod layout;
pub mod resource;
pub mod searchpaths;
pub mod vpk;

pub use elements::{ElementId, ElementSpec};
pub use layout::{ElementEdit, HudLayout, StylePatch};
