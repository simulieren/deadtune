//! System-level facts: power, Game Bar/Game Mode/graphics registry flags, memory
//! integrity, RAM sticks, overlays, game drive type.

use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sysinfo::{DiskKind, Disks, ProcessRefreshKind, ProcessesToUpdate, System};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, PowerGetActiveScheme, PowerReadFriendlyName, SYSTEM_POWER_STATUS,
};
use windows_sys::core::GUID;

use super::registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, Key};
use crate::winfps::facts::{DriveKind, MemoryStick, PowerFacts, PowerMode, PowerPlan};
use crate::winfps::parse::system as parse;

fn active_scheme() -> Option<GUID> {
    let mut ptr: *mut GUID = std::ptr::null_mut();
    // SAFETY: a null root key means the current user; `ptr` is a valid out pointer.
    let rc = unsafe { PowerGetActiveScheme(std::ptr::null_mut(), &mut ptr) };
    if rc != 0 || ptr.is_null() {
        return None;
    }
    // SAFETY: on success `ptr` points to a GUID allocated for us.
    let guid = unsafe { *ptr };
    // SAFETY: the GUID was allocated by PowerGetActiveScheme, which requires LocalFree.
    unsafe { LocalFree(ptr.cast()) };
    Some(guid)
}

fn friendly_name(scheme: &GUID) -> Option<String> {
    let mut size: u32 = 0;
    // SAFETY: a null buffer asks only for the size in bytes; the optional GUIDs are null.
    let rc = unsafe {
        PowerReadFriendlyName(
            std::ptr::null_mut(),
            scheme,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    // 234 is ERROR_MORE_DATA.
    if (rc != 0 && rc != 234) || size < 2 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `buf` has `size` writable bytes.
    let rc = unsafe {
        PowerReadFriendlyName(
            std::ptr::null_mut(),
            scheme,
            std::ptr::null(),
            std::ptr::null(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let name = String::from_utf16_lossy(&buf[..len]);
    (!name.is_empty()).then_some(name)
}

fn power_mode(on_battery: bool) -> Option<PowerMode> {
    let key = Key::open(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes",
    )?;
    let name = if on_battery {
        "ActiveOverlayDcPowerScheme"
    } else {
        "ActiveOverlayAcPowerScheme"
    };
    parse::power_mode(&key.string(name)?)
}

pub fn power() -> Option<PowerFacts> {
    let scheme = active_scheme()?;
    let guid = parse::guid_string(scheme.data1, scheme.data2, scheme.data3, scheme.data4);
    let plan = parse::power_plan(&guid)
        .unwrap_or_else(|| PowerPlan::Other(friendly_name(&scheme).unwrap_or(guid)));
    // SAFETY: SYSTEM_POWER_STATUS is plain integers, so all-zero is a valid value.
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: `status` is a valid, writable SYSTEM_POWER_STATUS for the duration of the call.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return None;
    }
    let on_battery = status.ACLineStatus == 0;
    Some(PowerFacts {
        plan,
        mode: power_mode(on_battery),
        on_battery,
        battery_saver: status.SystemStatusFlag == 1,
    })
}

pub fn background_recording() -> Option<bool> {
    let dvr = Key::open(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
    );
    let capture = dvr.as_ref().and_then(|k| k.dword("AppCaptureEnabled"));
    let historical = dvr
        .as_ref()
        .and_then(|k| k.dword("HistoricalCaptureEnabled"));
    let master = Key::open(HKEY_CURRENT_USER, r"System\GameConfigStore")
        .and_then(|k| k.dword("GameDVR_Enabled"));
    if capture.is_none() && historical.is_none() && master.is_none() {
        return None;
    }
    Some(historical == Some(1) && capture != Some(0) && master != Some(0))
}

pub fn game_mode() -> Option<bool> {
    let key = Key::open(HKEY_CURRENT_USER, r"Software\Microsoft\GameBar")?;
    // Windows default is on when the value is absent.
    Some(key.dword("AutoGameModeEnabled") != Some(0))
}

pub fn windowed_optimizations() -> Option<bool> {
    let key = Key::open(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\DirectX\UserGpuPreferences",
    )?;
    parse::swap_effect_upgrade(&key.string("DirectXUserGlobalSettings")?)
}

pub fn hags() -> Option<bool> {
    let key = Key::open(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
    )?;
    match key.dword("HwSchMode")? {
        2 => Some(true),
        1 => Some(false),
        _ => None,
    }
}

pub fn memory_integrity() -> Option<bool> {
    let key = Key::open(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
    )?;
    match key.dword("Enabled")? {
        1 => Some(true),
        0 => Some(false),
        _ => None,
    }
}

pub fn memory() -> Vec<MemoryStick> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const TIMEOUT: Duration = Duration::from_secs(5);
    let Ok(mut child) = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_PhysicalMemory | Select-Object Capacity,Speed,ConfiguredClockSpeed | ConvertTo-Json -Compress",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Vec::new();
    };
    // Drain on a thread so a full pipe cannot stall the child while we poll.
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    let finished = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if start.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let out = reader.join().unwrap_or_default();
    if !finished {
        return Vec::new();
    }
    parse::memory_sticks(&String::from_utf8_lossy(&out))
}

pub fn process_names() -> Vec<String> {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    system
        .processes()
        .values()
        .map(|p| p.name().to_string_lossy().into_owned())
        .collect()
}

pub fn drive_kind(path: &Path) -> Option<DriveKind> {
    let disks = Disks::new_with_refreshed_list();
    match game_disk(&disks, path)?.kind() {
        DiskKind::SSD => Some(DriveKind::Ssd),
        DiskKind::HDD => Some(DriveKind::Hdd),
        _ => None,
    }
}

pub fn free_mib(path: &Path) -> Option<u64> {
    let disks = Disks::new_with_refreshed_list();
    Some(game_disk(&disks, path)?.available_space() / (1024 * 1024))
}

fn game_disk<'a>(disks: &'a Disks, path: &Path) -> Option<&'a sysinfo::Disk> {
    let mounts: Vec<String> = disks
        .list()
        .iter()
        .map(|d| d.mount_point().to_string_lossy().into_owned())
        .collect();
    let index = parse::longest_mount(&path.to_string_lossy(), &mounts)?;
    disks.list().get(index)
}
