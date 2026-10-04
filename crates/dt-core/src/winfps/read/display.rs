//! Attached displays: current and highest refresh rate at the current resolution.

use windows_sys::Win32::Graphics::Gdi::{
    DEVMODEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS,
    EnumDisplayDevicesW, EnumDisplaySettingsW,
};

use crate::winfps::facts::Display;
use crate::winfps::parse::gpu::max_hz_for;

fn text(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn display_device(device: *const u16, index: u32) -> Option<DISPLAY_DEVICEW> {
    // SAFETY: DISPLAY_DEVICEW is plain integers and arrays, so all-zero is valid.
    let mut dd: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
    dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    // SAFETY: `device` is null or NUL-terminated; `dd` is a valid out struct with `cb` set.
    (unsafe { EnumDisplayDevicesW(device, index, &mut dd, 0) } != 0).then_some(dd)
}

fn mode(device: &[u16; 32], index: u32) -> Option<DEVMODEW> {
    let mut dm = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `device` is NUL-terminated; `dm` is a valid out struct with `dmSize` set.
    (unsafe { EnumDisplaySettingsW(device.as_ptr(), index, &mut dm) } != 0).then_some(dm)
}

pub fn displays() -> Vec<Display> {
    let mut out = Vec::new();
    for index in 0.. {
        let Some(adapter) = display_device(std::ptr::null(), index) else {
            break;
        };
        if adapter.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
            continue;
        }
        let Some(current) = mode(&adapter.DeviceName, ENUM_CURRENT_SETTINGS) else {
            continue;
        };
        let (width, height, current_hz) = (
            current.dmPelsWidth,
            current.dmPelsHeight,
            current.dmDisplayFrequency,
        );
        if current_hz <= 1 {
            continue;
        }
        let modes: Vec<(u32, u32, u32)> = (0..)
            .map_while(|i| mode(&adapter.DeviceName, i))
            .map(|m| (m.dmPelsWidth, m.dmPelsHeight, m.dmDisplayFrequency))
            .collect();
        let max_hz = max_hz_for(&modes, width, height)
            .unwrap_or(current_hz)
            .max(current_hz);
        let name = display_device(adapter.DeviceName.as_ptr(), 0)
            .map(|m| text(&m.DeviceString))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| text(&adapter.DeviceName));
        out.push(Display {
            name,
            current_hz,
            max_hz,
        });
    }
    out
}
