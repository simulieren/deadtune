//! Windows readers. Each returns what it could read and swallows errors into
//! `None`/empty: a failed read skips a check, it never fails the doctor run.

mod display;
mod gpu;
mod registry;
mod system;

use super::facts::WindowsFacts;
use crate::locate::GamePaths;

pub fn gather(paths: Option<&GamePaths>) -> WindowsFacts {
    let processes = system::process_names();
    WindowsFacts {
        gpus: gpu::gpus(),
        deadlock_gpu_preference: gpu::deadlock_gpu_preference(),
        power: system::power(),
        displays: display::displays(),
        background_recording: system::background_recording(),
        game_mode: system::game_mode(),
        windowed_optimizations: system::windowed_optimizations(),
        hags: system::hags(),
        memory_integrity: system::memory_integrity(),
        memory: system::memory(),
        overlays: Some(super::parse::system::match_overlays(&processes)),
        game_drive: paths.and_then(|p| system::drive_kind(&p.game_root)),
        game_drive_free_mib: paths.and_then(|p| system::free_mib(&p.game_root)),
        shader_processing: Some(super::parse::system::shader_processing(&processes)),
    }
}
