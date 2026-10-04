//! AC vs battery, for auto-switching profiles before launch.

#[cfg(any(target_os = "linux", test))]
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSource {
    Ac,
    Battery,
    Unknown,
}

#[cfg(windows)]
pub fn power_source() -> PowerSource {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    // SAFETY: SYSTEM_POWER_STATUS is plain integers, so all-zero is a valid value.
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: `status` is a valid, writable SYSTEM_POWER_STATUS for the duration of the call.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return PowerSource::Unknown;
    }
    match status.ACLineStatus {
        0 => PowerSource::Battery,
        1 => PowerSource::Ac,
        _ => PowerSource::Unknown,
    }
}

#[cfg(target_os = "linux")]
pub fn power_source() -> PowerSource {
    power_source_from_sysfs(Path::new("/sys/class/power_supply"))
}

#[cfg(target_os = "macos")]
pub fn power_source() -> PowerSource {
    match std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
    {
        Ok(out) if out.status.success() => parse_pmset_batt(&String::from_utf8_lossy(&out.stdout)),
        _ => PowerSource::Unknown,
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn power_source() -> PowerSource {
    PowerSource::Unknown
}

#[cfg(any(target_os = "linux", test))]
fn power_source_from_sysfs(power_supply_dir: &Path) -> PowerSource {
    let Ok(entries) = std::fs::read_dir(power_supply_dir) else {
        return PowerSource::Unknown;
    };
    let (mut has_mains, mut charger_online) = (false, false);
    let (mut discharging, mut battery_on_ac) = (false, false);
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        let read = |file: &str| {
            std::fs::read_to_string(dir.join(file))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        match read("type").as_str() {
            "Mains" => {
                has_mains = true;
                charger_online |= read("online") == "1";
            }
            // USB-C chargers; an idle port reports online 0, so only "online" counts.
            "USB" => charger_online |= read("online") == "1",
            // Mice and headsets report scope "Device"; only system batteries say anything about AC.
            "Battery" if read("scope") != "Device" => match read("status").as_str() {
                "Discharging" => discharging = true,
                "Charging" | "Full" | "Not charging" => battery_on_ac = true,
                _ => {}
            },
            _ => {}
        }
    }
    if charger_online {
        PowerSource::Ac
    } else if has_mains || discharging {
        PowerSource::Battery
    } else if battery_on_ac {
        PowerSource::Ac
    } else {
        PowerSource::Unknown
    }
}

#[cfg(any(target_os = "macos", test))]
fn parse_pmset_batt(output: &str) -> PowerSource {
    if output.contains("'AC Power'") {
        PowerSource::Ac
    } else if output.contains("'Battery Power'") {
        PowerSource::Battery
    } else {
        PowerSource::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supply(root: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, value) in files {
            std::fs::write(dir.join(file), format!("{value}\n")).unwrap();
        }
    }

    #[test]
    fn sysfs_mains_online_is_ac() {
        let root = tempfile::tempdir().unwrap();
        supply(root.path(), "AC", &[("type", "Mains"), ("online", "1")]);
        supply(
            root.path(),
            "BAT0",
            &[("type", "Battery"), ("status", "Charging")],
        );
        assert_eq!(power_source_from_sysfs(root.path()), PowerSource::Ac);
    }

    #[test]
    fn sysfs_mains_offline_with_discharging_battery_is_battery() {
        let root = tempfile::tempdir().unwrap();
        supply(root.path(), "ACAD", &[("type", "Mains"), ("online", "0")]);
        supply(
            root.path(),
            "BAT1",
            &[("type", "Battery"), ("status", "Discharging")],
        );
        assert_eq!(power_source_from_sysfs(root.path()), PowerSource::Battery);
    }

    #[test]
    fn sysfs_usb_c_charger_online_is_ac() {
        let root = tempfile::tempdir().unwrap();
        supply(
            root.path(),
            "ucsi-source-psy-USBC000:001",
            &[("type", "USB"), ("online", "1")],
        );
        supply(
            root.path(),
            "BAT0",
            &[("type", "Battery"), ("status", "Charging")],
        );
        assert_eq!(power_source_from_sysfs(root.path()), PowerSource::Ac);
    }

    #[test]
    fn sysfs_battery_only_uses_its_status() {
        let root = tempfile::tempdir().unwrap();
        supply(
            root.path(),
            "BAT0",
            &[("type", "Battery"), ("status", "Full")],
        );
        assert_eq!(power_source_from_sysfs(root.path()), PowerSource::Ac);
        supply(
            root.path(),
            "BAT0",
            &[("type", "Battery"), ("status", "Discharging")],
        );
        assert_eq!(power_source_from_sysfs(root.path()), PowerSource::Battery);
    }

    #[test]
    fn sysfs_ignores_peripheral_batteries_on_a_desktop() {
        let root = tempfile::tempdir().unwrap();
        supply(
            root.path(),
            "hidpp_battery_0",
            &[
                ("type", "Battery"),
                ("scope", "Device"),
                ("status", "Discharging"),
            ],
        );
        assert_eq!(power_source_from_sysfs(root.path()), PowerSource::Unknown);
    }

    #[test]
    fn sysfs_missing_or_empty_is_unknown() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(power_source_from_sysfs(root.path()), PowerSource::Unknown);
        assert_eq!(
            power_source_from_sysfs(&root.path().join("nope")),
            PowerSource::Unknown
        );
    }

    #[test]
    fn pmset_output_names_the_power_source() {
        let ac = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1234)\t100%; charged; 0:00 remaining present: true\n";
        let battery = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1234)\t87%; discharging; 5:12 remaining present: true\n";
        assert_eq!(parse_pmset_batt(ac), PowerSource::Ac);
        assert_eq!(parse_pmset_batt(battery), PowerSource::Battery);
        assert_eq!(parse_pmset_batt(""), PowerSource::Unknown);
    }
}
