//! HUD editing. Two levers, both outside the game process:
//! - HUD ConVars (crosshair, minimap icons, overhead bars) flow through the normal
//!   profile/apply pipeline; `convars` only supplies their catalog entries.
//! - Layout edits (move, scale, hide, fade) compile to CSS that is appended to the
//!   game's own compiled `hud.vcss_c` and shipped as one DeadTune-owned addon VPK.
//!   Experimental minimap icon colours, marker sizes and map opacity take the same
//!   path into `hud_minimap.vcss_c`.
//!
//! Pipeline: `HudLayout` -> `layout::compile` -> `HudPatch` -> `install::plan`
//! (reads the game's pak01, patches each style file via `resource`, rebuilds each
//! layout via `inject`, packs via `vpk`) -> `install::execute`.

pub mod apples_tunnels;
pub mod convars;
pub mod crc32;
pub mod css;
pub mod elements;
pub mod health_style;
pub mod inject;
pub mod install;
pub mod kv3;
pub mod layout;
pub mod minimap_colors;
pub mod minimap_style;
pub mod resource;
pub mod searchpaths;
pub mod topbar;
pub mod vpk;

pub use elements::{ElementId, ElementSpec};
pub use layout::{ElementEdit, HudLayout, HudPatch};
pub use minimap_colors::{Color, IconId};
pub use topbar::TopBarStyle;
