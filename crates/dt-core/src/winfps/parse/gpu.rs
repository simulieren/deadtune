//! Pure parsers for the GPU and display readers.

use crate::winfps::facts::{Date, GpuPreference};

/// Registry `DriverDate` is "M-D-YYYY" (some drivers use "/").
pub fn parse_driver_date(s: &str) -> Option<Date> {
    let mut parts = s.trim().split(['-', '/']);
    let month: u8 = parts.next()?.trim().parse().ok()?;
    let day: u8 = parts.next()?.trim().parse().ok()?;
    let year: u16 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if year < 1990 {
        return None;
    }
    Some(Date { year, month, day })
}

/// Reads `GpuPreference=N;` out of a `UserGpuPreferences` value (other `key=value;`
/// pairs may surround it). 0 Default, 1 PowerSaving, 2 HighPerformance.
pub fn parse_gpu_preference(s: &str) -> Option<GpuPreference> {
    let value = s.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("GpuPreference")
            .then(|| value.trim())
    })?;
    match value {
        "0" => Some(GpuPreference::Default),
        "1" => Some(GpuPreference::PowerSaving),
        "2" => Some(GpuPreference::HighPerformance),
        _ => None,
    }
}

/// True for the `UserGpuPreferences` value name of the game exe.
pub fn is_deadlock_exe(path: &str) -> bool {
    let path = path.trim_end().replace('/', "\\").to_ascii_lowercase();
    path.ends_with("\\deadlock.exe")
}

/// PCI vendor id from a hardware id like `PCI\VEN_10DE&DEV_2684&SUBSYS_...`.
pub fn vendor_from_hardware_id(id: &str) -> Option<u16> {
    let upper = id.to_ascii_uppercase();
    if !upper.starts_with("PCI\\") {
        return None;
    }
    let hex = upper.split("VEN_").nth(1)?.get(..4)?;
    u16::from_str_radix(hex, 16).ok()
}

/// Software and remote adapters that are not a real GPU.
pub fn is_software_adapter(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "microsoft basic render",
        "microsoft basic display",
        "microsoft remote display",
        "microsoft hyper-v video",
    ]
    .iter()
    .any(|s| name.contains(s))
}

/// Bytes to MiB; 0 means the driver did not report it.
pub fn vram_mib(bytes: u64) -> Option<u64> {
    (bytes > 0).then_some(bytes / (1024 * 1024))
}

/// Highest usable refresh rate among `(width, height, hz)` modes at this resolution.
/// 0 and 1 mean "hardware default" and are skipped.
pub fn max_hz_for(modes: &[(u32, u32, u32)], width: u32, height: u32) -> Option<u32> {
    modes
        .iter()
        .filter(|&&(w, h, hz)| w == width && h == height && hz > 1)
        .map(|&(_, _, hz)| hz)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_date() {
        assert_eq!(
            parse_driver_date("10-4-2026"),
            Some(Date {
                year: 2026,
                month: 10,
                day: 4
            })
        );
        assert_eq!(
            parse_driver_date("1/15/2024"),
            Some(Date {
                year: 2024,
                month: 1,
                day: 15
            })
        );
        for bad in [
            "",
            "garbage",
            "13-1-2024",
            "1-32-2024",
            "1-1",
            "1-1-2024-5",
            "1-1-0",
        ] {
            assert_eq!(parse_driver_date(bad), None, "{bad}");
        }
    }

    #[test]
    fn preference() {
        assert_eq!(
            parse_gpu_preference("GpuPreference=2;"),
            Some(GpuPreference::HighPerformance)
        );
        assert_eq!(
            parse_gpu_preference("GpuPreference=1;"),
            Some(GpuPreference::PowerSaving)
        );
        assert_eq!(
            parse_gpu_preference("GpuPreference=0;"),
            Some(GpuPreference::Default)
        );
        assert_eq!(
            parse_gpu_preference("GpuPreference=2"),
            Some(GpuPreference::HighPerformance)
        );
        assert_eq!(
            parse_gpu_preference("AutoHDREnable=2097;GpuPreference=2;"),
            Some(GpuPreference::HighPerformance)
        );
        assert_eq!(parse_gpu_preference("AutoHDREnable=2097;"), None);
        assert_eq!(parse_gpu_preference("GpuPreference=9;"), None);
        assert_eq!(parse_gpu_preference(""), None);
    }

    #[test]
    fn deadlock_exe() {
        let p = r"D:\SteamLibrary\steamapps\common\Deadlock\game\bin\win64\deadlock.exe";
        assert!(is_deadlock_exe(p));
        assert!(is_deadlock_exe(&p.to_uppercase()));
        assert!(!is_deadlock_exe(r"C:\Games\notdeadlock.exe"));
        assert!(!is_deadlock_exe("DirectXUserGlobalSettings"));
    }

    #[test]
    fn vendor() {
        assert_eq!(
            vendor_from_hardware_id(r"PCI\VEN_10DE&DEV_2684&SUBSYS_16F11043"),
            Some(0x10DE)
        );
        assert_eq!(
            vendor_from_hardware_id(r"PCI\VEN_1002&DEV_73BF"),
            Some(0x1002)
        );
        assert_eq!(
            vendor_from_hardware_id(r"pci\ven_8086&dev_9a49"),
            Some(0x8086)
        );
        assert_eq!(vendor_from_hardware_id(r"ROOT\BasicDisplay"), None);
        assert_eq!(vendor_from_hardware_id(r"PCI\VEN_"), None);
        assert_eq!(vendor_from_hardware_id(""), None);
    }

    #[test]
    fn software() {
        assert!(is_software_adapter("Microsoft Basic Render Driver"));
        assert!(is_software_adapter("Microsoft Basic Display Adapter"));
        assert!(is_software_adapter("Microsoft Remote Display Adapter"));
        assert!(!is_software_adapter("NVIDIA GeForce RTX 4070"));
    }

    #[test]
    fn vram() {
        assert_eq!(vram_mib(0), None);
        assert_eq!(vram_mib(8 * 1024 * 1024 * 1024), Some(8192));
    }

    #[test]
    fn max_hz() {
        let modes = [
            (1920, 1080, 60),
            (1920, 1080, 144),
            (1920, 1080, 1),
            (2560, 1440, 240),
        ];
        assert_eq!(max_hz_for(&modes, 1920, 1080), Some(144));
        assert_eq!(max_hz_for(&modes, 2560, 1440), Some(240));
        assert_eq!(max_hz_for(&modes, 800, 600), None);
        assert_eq!(
            max_hz_for(&[(1920, 1080, 0), (1920, 1080, 1)], 1920, 1080),
            None
        );
    }
}
