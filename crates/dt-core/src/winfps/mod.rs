//! Read-only Windows checks for settings that can cost FPS (power plan, GPU choice on
//! laptops, refresh rate, background recording, RAM, overlays). DeadTune never changes
//! Windows settings; each warning names the setting and links to its Settings page.
//!
//! `read` gathers `WindowsFacts` from the registry, Win32 APIs and the process list
//! (Windows only). `evaluate` turns facts into doctor checks and is pure, so it is
//! tested on every platform.

pub mod evaluate;
pub mod facts;
pub mod parse;
#[cfg(windows)]
mod read;

pub use facts::WindowsFacts;

use crate::doctor::Check;
use crate::locate::GamePaths;

/// Empty on other platforms.
pub fn checks(paths: Option<&GamePaths>) -> Vec<Check> {
    #[cfg(windows)]
    {
        let today = facts::Date::today();
        evaluate::checks(&read::gather(paths), today)
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Vec::new()
    }
}
