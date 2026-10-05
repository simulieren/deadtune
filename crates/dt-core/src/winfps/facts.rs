//! What `read` found on this PC. Every field is optional or a list: a value that could
//! not be read stays `None`/empty and its check is skipped, never guessed.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowsFacts {
    /// Active display adapters, software renderers excluded.
    pub gpus: Vec<Gpu>,
    /// Per-app GPU choice for `deadlock.exe` in Settings > Display > Graphics.
    /// `None` when no entry exists (Windows then decides, often the integrated GPU).
    pub deadlock_gpu_preference: Option<GpuPreference>,
    pub power: Option<PowerFacts>,
    pub displays: Vec<Display>,
    /// Xbox Game Bar background recording ("Record what happened").
    pub background_recording: Option<bool>,
    pub game_mode: Option<bool>,
    /// Windows 11 "Optimizations for windowed games".
    pub windowed_optimizations: Option<bool>,
    /// Hardware-accelerated GPU scheduling.
    pub hags: Option<bool>,
    /// Core isolation > Memory integrity (HVCI).
    pub memory_integrity: Option<bool>,
    /// Installed RAM sticks; empty when unknown.
    pub memory: Vec<MemoryStick>,
    /// Known overlay or capture programs running now, by display name (`OVERLAYS`);
    /// `None` when the process list was not read.
    pub overlays: Option<Vec<&'static str>>,
    /// Drive the Deadlock folder is on.
    pub game_drive: Option<DriveKind>,
    /// Free space on that drive, in MiB.
    pub game_drive_free_mib: Option<u64>,
    /// Steam's background shader processing (`fossilize_replay.exe`) is running;
    /// `None` when the process list was not read.
    pub shader_processing: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gpu {
    pub name: String,
    /// PCI vendor id: 0x10DE NVIDIA, 0x1002 AMD, 0x8086 Intel.
    pub vendor_id: Option<u16>,
    /// Dedicated video memory in MiB, when the driver reports it.
    pub vram_mib: Option<u64>,
    pub driver_date: Option<Date>,
    pub driver_version: Option<String>,
}

pub const VENDOR_NVIDIA: u16 = 0x10DE;
pub const VENDOR_AMD: u16 = 0x1002;
pub const VENDOR_INTEL: u16 = 0x8086;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuPreference {
    /// "Let Windows decide" (`GpuPreference=0`).
    Default,
    PowerSaving,
    HighPerformance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerFacts {
    pub plan: PowerPlan,
    /// Windows 11 power mode slider (an overlay on the plan); `None` when not available.
    pub mode: Option<PowerMode>,
    pub on_battery: bool,
    pub battery_saver: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PowerPlan {
    PowerSaver,
    Balanced,
    HighPerformance,
    UltimatePerformance,
    /// A vendor or user plan; the friendly name when it could be read.
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerMode {
    BestPowerEfficiency,
    Balanced,
    BestPerformance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub name: String,
    pub current_hz: u32,
    /// Highest refresh rate the display offers at its current resolution.
    pub max_hz: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryStick {
    pub capacity_mib: u64,
    /// Rated speed in MT/s (WMI `Speed`).
    pub rated_mts: Option<u32>,
    /// Speed it runs at in MT/s (WMI `ConfiguredClockSpeed`).
    pub configured_mts: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveKind {
    Ssd,
    Hdd,
}

/// Process name (case-insensitive) to display name. Presence is reported, not judged.
pub const OVERLAYS: &[(&str, &str)] = &[
    ("Discord.exe", "Discord"),
    ("NVIDIA Overlay.exe", "NVIDIA overlay"),
    ("nvcontainer.exe", "NVIDIA App / GeForce Experience"),
    ("RTSS.exe", "RivaTuner Statistics Server"),
    ("MSIAfterburner.exe", "MSI Afterburner"),
    ("obs64.exe", "OBS Studio"),
    ("GameBar.exe", "Xbox Game Bar"),
    ("Medal.exe", "Medal"),
    ("Overwolf.exe", "Overwolf"),
    ("RadeonSoftware.exe", "AMD Software overlay"),
];

/// A calendar date, for driver age.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// Days since 1970-01-01 (proleptic Gregorian).
    pub fn days(self) -> i64 {
        let (y, m, d) = (
            i64::from(self.year),
            i64::from(self.month),
            i64::from(self.day),
        );
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    pub fn from_days(days: i64) -> Date {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
        Date { year, month, day }
    }

    pub fn today() -> Date {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Date::from_days((secs / 86_400) as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_days_round_trip() {
        for (date, days) in [
            (
                Date {
                    year: 1970,
                    month: 1,
                    day: 1,
                },
                0,
            ),
            (
                Date {
                    year: 2000,
                    month: 3,
                    day: 1,
                },
                11_017,
            ),
            (
                Date {
                    year: 2026,
                    month: 10,
                    day: 4,
                },
                20_730,
            ),
        ] {
            assert_eq!(date.days(), days, "{date:?}");
            assert_eq!(Date::from_days(days), date);
        }
    }
}
