//! Facts to doctor checks. Pure: no OS calls, so it is tested on every platform.

use super::facts::{
    Date, Display, DriveKind, Gpu, GpuPreference, MemoryStick, PowerFacts, PowerMode, PowerPlan,
    VENDOR_AMD, VENDOR_INTEL, VENDOR_NVIDIA, WindowsFacts,
};
use crate::doctor::{Check, pass, warn};

const DRIVER_MAX_AGE_DAYS: i64 = 180;

/// One check per fact that was read, in the order of the research table
/// (GPU choice, power, refresh rate, recording, RAM, overlays, driver age, Game Mode,
/// memory integrity, windowed optimizations, HAGS, drive).
pub fn checks(facts: &WindowsFacts, today: Date) -> Vec<Check> {
    [
        gpu_choice(facts),
        power(facts.power.as_ref()),
        refresh_rate(&facts.displays),
        background_recording(facts.background_recording),
        ram(&facts.memory),
        overlays(facts.overlays.as_deref()),
        gpu_driver(&facts.gpus, today),
        game_mode(facts.game_mode),
        memory_integrity(facts.memory_integrity),
        windowed_optimizations(facts.windowed_optimizations),
        hags(facts.hags),
        game_drive(facts.game_drive),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn with_link(check: Check, link: &'static str) -> Check {
    Check {
        link: Some(link),
        ..check
    }
}

fn is_discrete(gpu: &Gpu) -> bool {
    match gpu.vendor_id {
        Some(VENDOR_NVIDIA) => true,
        Some(VENDOR_AMD) => gpu.vram_mib.is_none_or(|v| v >= 2048),
        _ => false,
    }
}

fn gpu_choice(facts: &WindowsFacts) -> Option<Check> {
    if facts.gpus.len() < 2 || !facts.gpus.iter().any(is_discrete) {
        return None;
    }
    const NAME: &str = "GPU for Deadlock";
    if facts.deadlock_gpu_preference == Some(GpuPreference::HighPerformance) {
        return Some(pass(
            NAME,
            "Deadlock is set to use the high-performance GPU",
        ));
    }
    Some(with_link(
        warn(
            NAME,
            "this PC has more than one GPU and Windows may run Deadlock on the integrated one",
            "Open Settings > System > Display > Graphics, add deadlock.exe \
             (Steam library > steamapps\\common\\Deadlock\\game\\bin\\win64\\deadlock.exe), \
             then Options > High performance.",
        ),
        "ms-settings:display-advancedgraphics",
    ))
}

fn plan_name(plan: &PowerPlan) -> &str {
    match plan {
        PowerPlan::PowerSaver => "Power saver",
        PowerPlan::Balanced => "Balanced",
        PowerPlan::HighPerformance => "High performance",
        PowerPlan::UltimatePerformance => "Ultimate Performance",
        PowerPlan::Other(name) => name,
    }
}

fn mode_name(mode: PowerMode) -> &'static str {
    match mode {
        PowerMode::BestPowerEfficiency => "Best power efficiency",
        PowerMode::Balanced => "Balanced",
        PowerMode::BestPerformance => "Best performance",
    }
}

fn power(power: Option<&PowerFacts>) -> Option<Check> {
    const NAME: &str = "Power plan";
    let p = power?;
    let mut problems: Vec<&str> = Vec::new();
    let mut fixes: Vec<&str> = Vec::new();
    if p.battery_saver {
        problems.push("battery saver is on");
        fixes.push("Turn off battery saver in Settings > System > Power & battery.");
    }
    if p.on_battery {
        problems.push("the laptop is running on battery");
        fixes.push("Plug in the charger while playing.");
    }
    if p.plan == PowerPlan::PowerSaver {
        problems.push("the Power saver plan is active");
        fixes.push("Pick Balanced or Best performance under Settings > System > Power & battery > Power mode.");
    }
    if p.mode == Some(PowerMode::BestPowerEfficiency) {
        problems.push("the power mode is Best power efficiency");
        fixes.push("Set Power mode to Balanced or Best performance under Settings > System > Power & battery.");
    }
    let mut state = plan_name(&p.plan).to_string();
    if let Some(mode) = p.mode {
        state.push_str(&format!(", mode {}", mode_name(mode)));
    }
    if problems.is_empty() {
        return Some(pass(NAME, state));
    }
    Some(with_link(
        warn(
            NAME,
            format!("{state}; {}, which can lower FPS", problems.join(", ")),
            &fixes.join(" "),
        ),
        "ms-settings:powersleep",
    ))
}

fn refresh_rate(displays: &[Display]) -> Option<Check> {
    const NAME: &str = "Refresh rate";
    if displays.is_empty() {
        return None;
    }
    let slow: Vec<String> = displays
        .iter()
        // 59 vs 60 Hz style pairs are the same panel rate, so only a real step up counts.
        .filter(|d| u64::from(d.max_hz) * 10 > u64::from(d.current_hz) * 11)
        .map(|d| {
            format!(
                "{} runs at {} Hz but supports {} Hz",
                d.name, d.current_hz, d.max_hz
            )
        })
        .collect();
    if slow.is_empty() {
        let list: Vec<String> = displays
            .iter()
            .map(|d| format!("{} at {} Hz", d.name, d.current_hz))
            .collect();
        return Some(pass(NAME, list.join(", ")));
    }
    Some(with_link(
        warn(
            NAME,
            slow.join("; "),
            "Open Settings > System > Display > Advanced display and pick the highest rate \
             under Choose a refresh rate.",
        ),
        "ms-settings:display",
    ))
}

fn background_recording(on: Option<bool>) -> Option<Check> {
    const NAME: &str = "Background recording";
    Some(match on? {
        true => with_link(
            warn(
                NAME,
                "Xbox Game Bar keeps recording in the background, which uses GPU and disk",
                "Open Settings > Gaming > Captures and turn off Record what happened.",
            ),
            "ms-settings:gaming-gamedvr",
        ),
        false => pass(NAME, "off"),
    })
}

fn ram(memory: &[MemoryStick]) -> Option<Check> {
    const NAME: &str = "RAM";
    if memory.is_empty() {
        return None;
    }
    let total_mib: u64 = memory.iter().map(|s| s.capacity_mib).sum();
    let total = format!("{} GiB", total_mib / 1024);
    let mut problems: Vec<String> = Vec::new();
    let mut fixes: Vec<&str> = Vec::new();
    if memory.len() == 1 {
        problems.push(
            "only one memory stick was found, so it may run in single channel. Two matching \
             sticks are often noticeably faster in CPU-heavy games like Deadlock \
             (RAM soldered to a laptop board can also report as one stick)"
                .to_string(),
        );
        fixes.push(
            "If your board has a free slot, adding a second matching stick can help. \
             Skip this if your RAM is soldered on.",
        );
    }
    let slow = memory
        .iter()
        .any(|s| match (s.configured_mts, s.rated_mts) {
            (Some(cfg), Some(rated)) => u64::from(cfg) * 10 < u64::from(rated) * 9,
            _ => false,
        });
    if slow {
        problems
            .push("the RAM runs well below its rated speed, so XMP/EXPO may be off".to_string());
        fixes.push("Enable XMP or EXPO in your BIOS/UEFI memory settings.");
    }
    let speed = memory
        .iter()
        .filter_map(|s| s.configured_mts.or(s.rated_mts))
        .max();
    let mut state = format!(
        "{total} in {} stick{}",
        memory.len(),
        if memory.len() == 1 { "" } else { "s" }
    );
    if let Some(mts) = speed {
        state.push_str(&format!(", {mts} MT/s"));
    }
    if problems.is_empty() {
        return Some(pass(NAME, state));
    }
    Some(warn(
        NAME,
        format!("{state}; {}", problems.join("; ")),
        &fixes.join(" "),
    ))
}

fn overlays(running: Option<&[&'static str]>) -> Option<Check> {
    const NAME: &str = "Overlays";
    let running = running?;
    let list = running.join(", ");
    Some(match running.len() {
        0 => pass(NAME, "none of the known overlays are running"),
        1 | 2 => pass(NAME, format!("running: {list}")),
        _ => warn(
            NAME,
            format!(
                "running: {list}. Overlays and capture programs can each add a little GPU or CPU load"
            ),
            "Close the ones you do not use while playing.",
        ),
    })
}

fn driver_vendor_fix(gpu: &Gpu) -> &'static str {
    match gpu.vendor_id {
        Some(VENDOR_NVIDIA) => "Open the NVIDIA App and install the newest Game Ready driver.",
        Some(VENDOR_AMD) => "Open AMD Software: Adrenalin Edition and check for driver updates.",
        Some(VENDOR_INTEL) => {
            "Open Intel Graphics Software (or Arc Control) and check for driver updates."
        }
        _ => "Download the newest driver from your GPU maker's website.",
    }
}

fn gpu_driver(gpus: &[Gpu], today: Date) -> Option<Check> {
    const NAME: &str = "GPU driver";
    let discrete = gpus
        .iter()
        .filter(|g| is_discrete(g) && g.driver_date.is_some());
    let gpu = match gpus {
        [only] => only,
        _ => discrete.max_by_key(|g| g.driver_date)?,
    };
    let date = gpu.driver_date?;
    let version = gpu
        .driver_version
        .as_deref()
        .map_or(String::new(), |v| format!(" {v}"));
    let state = format!(
        "{}: driver{version} from {:04}-{:02}-{:02}",
        gpu.name, date.year, date.month, date.day
    );
    let age = today.days() - date.days();
    if age > DRIVER_MAX_AGE_DAYS {
        return Some(warn(
            NAME,
            format!(
                "{state}, about {} months old. Newer drivers often include fixes for newer games",
                age / 30
            ),
            driver_vendor_fix(gpu),
        ));
    }
    Some(pass(NAME, state))
}

fn game_mode(on: Option<bool>) -> Option<Check> {
    const NAME: &str = "Game Mode";
    Some(match on? {
        false => with_link(
            warn(
                NAME,
                "Game Mode is off. Windows can use it to keep background tasks from competing with a game",
                "Open Settings > Gaming > Game Mode and turn it on.",
            ),
            "ms-settings:gaming-gamemode",
        ),
        true => pass(NAME, "on"),
    })
}

fn memory_integrity(on: Option<bool>) -> Option<Check> {
    const NAME: &str = "Memory integrity";
    Some(match on? {
        true => pass(
            NAME,
            "on. This security feature can cost a few percent on some CPUs; DeadTune does not suggest turning it off.",
        ),
        false => pass(NAME, "off"),
    })
}

fn windowed_optimizations(on: Option<bool>) -> Option<Check> {
    const NAME: &str = "Windowed game optimizations";
    Some(match on? {
        false => with_link(
            warn(
                NAME,
                "off. This Windows 11 setting can help latency when a game runs borderless or windowed",
                "Open Settings > System > Display > Graphics > Change default graphics settings \
                 and turn on Optimizations for windowed games.",
            ),
            "ms-settings:display-advancedgraphics",
        ),
        true => pass(NAME, "on"),
    })
}

fn hags(on: Option<bool>) -> Option<Check> {
    const NAME: &str = "GPU scheduling";
    Some(pass(
        NAME,
        if on? {
            "hardware-accelerated GPU scheduling is on"
        } else {
            "hardware-accelerated GPU scheduling is off"
        },
    ))
}

fn game_drive(kind: Option<DriveKind>) -> Option<Check> {
    const NAME: &str = "Game drive";
    Some(match kind? {
        DriveKind::Hdd => warn(
            NAME,
            "Deadlock is on a hard drive, which can mean longer loads and stutter when assets stream in",
            "Move the game to an SSD: Steam > Settings > Storage, pick the SSD, then move Deadlock.",
        ),
        DriveKind::Ssd => pass(NAME, "Deadlock is on an SSD"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::CheckStatus;

    const TODAY: Date = Date {
        year: 2026,
        month: 10,
        day: 4,
    };

    fn gpu(name: &str, vendor: u16, vram: Option<u64>, date: Option<Date>) -> Gpu {
        Gpu {
            name: name.into(),
            vendor_id: Some(vendor),
            vram_mib: vram,
            driver_date: date,
            driver_version: Some("1.2.3".into()),
        }
    }

    fn only(facts: &WindowsFacts) -> Check {
        let mut c = checks(facts, TODAY);
        assert_eq!(c.len(), 1, "{c:?}");
        c.remove(0)
    }

    fn power_facts(plan: PowerPlan) -> PowerFacts {
        PowerFacts {
            plan,
            mode: None,
            on_battery: false,
            battery_saver: false,
        }
    }

    #[test]
    fn default_facts_produce_no_checks() {
        assert!(checks(&WindowsFacts::default(), TODAY).is_empty());
    }

    #[test]
    fn gpu_choice_rules() {
        let hybrid = vec![
            gpu("Intel UHD", VENDOR_INTEL, None, None),
            gpu("RTX 4060", VENDOR_NVIDIA, Some(8192), None),
        ];
        let mut f = WindowsFacts {
            gpus: hybrid.clone(),
            ..Default::default()
        };
        // driver check is skipped: no dates
        let c = only(&f);
        assert_eq!(c.name, "GPU for Deadlock");
        assert_eq!(c.status, CheckStatus::Warn);
        assert_eq!(c.link, Some("ms-settings:display-advancedgraphics"));
        f.deadlock_gpu_preference = Some(GpuPreference::PowerSaving);
        assert_eq!(only(&f).status, CheckStatus::Warn);
        f.deadlock_gpu_preference = Some(GpuPreference::HighPerformance);
        assert_eq!(only(&f).status, CheckStatus::Pass);
        // single GPU, or two integrated GPUs: skipped
        f.gpus.truncate(1);
        assert!(
            checks(&f, TODAY)
                .iter()
                .all(|c| c.name != "GPU for Deadlock")
        );
        f.gpus = vec![
            gpu("Intel UHD", VENDOR_INTEL, None, None),
            gpu("Radeon 780M", VENDOR_AMD, Some(512), None),
        ];
        assert!(checks(&f, TODAY).is_empty());
        f.gpus[1].vram_mib = Some(2048);
        assert_eq!(only(&f).name, "GPU for Deadlock");
        f.gpus[1].vram_mib = None;
        assert_eq!(only(&f).name, "GPU for Deadlock");
    }

    #[test]
    fn power_rules() {
        let mut f = WindowsFacts {
            power: Some(power_facts(PowerPlan::Balanced)),
            ..Default::default()
        };
        let c = only(&f);
        assert_eq!(
            (c.name, c.status, c.link),
            ("Power plan", CheckStatus::Pass, None)
        );
        assert!(c.detail.contains("Balanced"));
        f.power = Some(power_facts(PowerPlan::HighPerformance));
        assert_eq!(only(&f).status, CheckStatus::Pass);
        f.power = Some(power_facts(PowerPlan::PowerSaver));
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Warn);
        assert_eq!(c.link, Some("ms-settings:powersleep"));
        let mut p = power_facts(PowerPlan::Balanced);
        p.on_battery = true;
        f.power = Some(p);
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Warn);
        assert!(c.fix.unwrap().contains("Plug in"));
        let mut p = power_facts(PowerPlan::Balanced);
        p.battery_saver = true;
        f.power = Some(p);
        assert_eq!(only(&f).status, CheckStatus::Warn);
        let mut p = power_facts(PowerPlan::Balanced);
        p.mode = Some(PowerMode::BestPowerEfficiency);
        f.power = Some(p);
        assert_eq!(only(&f).status, CheckStatus::Warn);
        let mut p = power_facts(PowerPlan::Balanced);
        p.mode = Some(PowerMode::BestPerformance);
        f.power = Some(p);
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Pass);
        assert!(c.detail.contains("Best performance"));
    }

    #[test]
    fn refresh_rate_rules() {
        let d = |hz, max| Display {
            name: "DELL S2721DGF".into(),
            current_hz: hz,
            max_hz: max,
        };
        let mut f = WindowsFacts {
            displays: vec![d(60, 165)],
            ..Default::default()
        };
        let c = only(&f);
        assert_eq!(
            (c.name, c.status, c.link),
            (
                "Refresh rate",
                CheckStatus::Warn,
                Some("ms-settings:display")
            )
        );
        assert!(
            c.detail
                .contains("DELL S2721DGF runs at 60 Hz but supports 165 Hz")
        );
        f.displays = vec![d(59, 60)];
        assert_eq!(only(&f).status, CheckStatus::Pass);
        f.displays = vec![d(144, 165)];
        assert_eq!(only(&f).status, CheckStatus::Warn);
        f.displays = vec![d(165, 165)];
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Pass);
        assert_eq!(c.link, None);
    }

    #[test]
    fn background_recording_rules() {
        let mut f = WindowsFacts {
            background_recording: Some(true),
            ..Default::default()
        };
        let c = only(&f);
        assert_eq!(
            (c.name, c.status, c.link),
            (
                "Background recording",
                CheckStatus::Warn,
                Some("ms-settings:gaming-gamedvr")
            )
        );
        f.background_recording = Some(false);
        assert_eq!(only(&f).status, CheckStatus::Pass);
    }

    fn stick(rated: Option<u32>, cfg: Option<u32>) -> MemoryStick {
        MemoryStick {
            capacity_mib: 16384,
            rated_mts: rated,
            configured_mts: cfg,
        }
    }

    #[test]
    fn ram_rules() {
        let mut f = WindowsFacts {
            memory: vec![stick(Some(6000), Some(6000)), stick(Some(6000), Some(6000))],
            ..Default::default()
        };
        let c = only(&f);
        assert_eq!((c.name, c.status, c.link), ("RAM", CheckStatus::Pass, None));
        assert!(c.detail.contains("32 GiB") && c.detail.contains("6000"));
        // exactly 10% below is fine, just past it warns
        f.memory = vec![stick(Some(6000), Some(5400)), stick(Some(6000), Some(5400))];
        assert_eq!(only(&f).status, CheckStatus::Pass);
        f.memory[1].configured_mts = Some(5399);
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Warn);
        assert!(c.fix.unwrap().contains("XMP or EXPO"));
        assert_eq!(c.link, None);
        // unknown speeds never warn
        f.memory = vec![stick(None, Some(2133)), stick(Some(3200), None)];
        assert_eq!(only(&f).status, CheckStatus::Pass);
        // single stick
        f.memory = vec![stick(Some(5600), Some(5600))];
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Warn);
        assert!(c.detail.contains("may run in single channel"));
    }

    #[test]
    fn overlay_rules() {
        let mut f = WindowsFacts {
            overlays: Some(vec![]),
            ..Default::default()
        };
        let c = only(&f);
        assert_eq!((c.name, c.status), ("Overlays", CheckStatus::Pass));
        assert!(c.detail.contains("none of the known overlays"));
        f.overlays = Some(vec!["Discord", "OBS Studio"]);
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Pass);
        assert!(c.detail.contains("Discord, OBS Studio"));
        f.overlays = Some(vec!["Discord", "OBS Studio", "Medal"]);
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Warn);
        assert!(c.fix.is_some());
    }

    fn driver_facts(g: Vec<Gpu>) -> WindowsFacts {
        WindowsFacts {
            gpus: g,
            ..Default::default()
        }
    }

    #[test]
    fn driver_age_boundary() {
        let day = |offset: i64| Date::from_days(TODAY.days() - offset);
        let f = driver_facts(vec![gpu(
            "RTX 4060",
            VENDOR_NVIDIA,
            Some(8192),
            Some(day(180)),
        )]);
        let c = only(&f);
        assert_eq!((c.name, c.status), ("GPU driver", CheckStatus::Pass));
        assert!(c.detail.contains("1.2.3"));
        let f = driver_facts(vec![gpu(
            "RTX 4060",
            VENDOR_NVIDIA,
            Some(8192),
            Some(day(181)),
        )]);
        let c = only(&f);
        assert_eq!(c.status, CheckStatus::Warn);
        assert!(c.fix.unwrap().contains("NVIDIA App"));
        assert_eq!(c.link, None);
        let f = driver_facts(vec![gpu(
            "Radeon RX 7800",
            VENDOR_AMD,
            Some(16384),
            Some(day(400)),
        )]);
        assert!(only(&f).fix.unwrap().contains("Adrenalin"));
        let f = driver_facts(vec![gpu(
            "Arc A750",
            VENDOR_INTEL,
            Some(8192),
            Some(day(400)),
        )]);
        assert!(only(&f).fix.unwrap().contains("Intel Graphics Software"));
        // unknown date: skipped
        let f = driver_facts(vec![gpu("RTX 4060", VENDOR_NVIDIA, Some(8192), None)]);
        assert!(checks(&f, TODAY).is_empty());
    }

    #[test]
    fn driver_check_targets_discrete_gpu() {
        let day = |offset: i64| Date::from_days(TODAY.days() - offset);
        let mut f = driver_facts(vec![
            gpu("Intel UHD", VENDOR_INTEL, None, Some(day(900))),
            gpu("RTX 4060", VENDOR_NVIDIA, Some(8192), Some(day(10))),
        ]);
        f.deadlock_gpu_preference = Some(GpuPreference::HighPerformance);
        let c = checks(&f, TODAY);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].name, "GPU driver");
        assert_eq!(c[1].status, CheckStatus::Pass);
        assert!(c[1].detail.contains("RTX 4060"));
    }

    #[test]
    fn game_mode_rules() {
        let mut f = WindowsFacts {
            game_mode: Some(false),
            ..Default::default()
        };
        let c = only(&f);
        assert_eq!(
            (c.name, c.status, c.link),
            (
                "Game Mode",
                CheckStatus::Warn,
                Some("ms-settings:gaming-gamemode")
            )
        );
        f.game_mode = Some(true);
        assert_eq!(only(&f).status, CheckStatus::Pass);
    }

    #[test]
    fn memory_integrity_never_warns() {
        for v in [true, false] {
            let f = WindowsFacts {
                memory_integrity: Some(v),
                ..Default::default()
            };
            let c = only(&f);
            assert_eq!(
                (c.name, c.status, c.fix, c.link),
                ("Memory integrity", CheckStatus::Pass, None, None)
            );
        }
        let f = WindowsFacts {
            memory_integrity: Some(true),
            ..Default::default()
        };
        assert!(only(&f).detail.contains("does not suggest turning it off"));
    }

    #[test]
    fn windowed_optimizations_rules() {
        let mut f = WindowsFacts {
            windowed_optimizations: Some(false),
            ..Default::default()
        };
        let c = only(&f);
        assert_eq!(
            (c.name, c.status, c.link),
            (
                "Windowed game optimizations",
                CheckStatus::Warn,
                Some("ms-settings:display-advancedgraphics")
            )
        );
        f.windowed_optimizations = Some(true);
        assert_eq!(only(&f).status, CheckStatus::Pass);
    }

    #[test]
    fn hags_never_warns() {
        for v in [true, false] {
            let f = WindowsFacts {
                hags: Some(v),
                ..Default::default()
            };
            let c = only(&f);
            assert_eq!(
                (c.name, c.status, c.fix),
                ("GPU scheduling", CheckStatus::Pass, None)
            );
        }
    }

    #[test]
    fn game_drive_rules() {
        let mut f = WindowsFacts {
            game_drive: Some(DriveKind::Hdd),
            ..Default::default()
        };
        let c = only(&f);
        assert_eq!(
            (c.name, c.status, c.link),
            ("Game drive", CheckStatus::Warn, None)
        );
        assert!(c.fix.unwrap().contains("Steam > Settings > Storage"));
        f.game_drive = Some(DriveKind::Ssd);
        assert_eq!(only(&f).status, CheckStatus::Pass);
    }

    #[test]
    fn checks_come_in_the_documented_order() {
        let f = WindowsFacts {
            gpus: vec![
                gpu("Intel UHD", VENDOR_INTEL, None, None),
                gpu(
                    "RTX 4060",
                    VENDOR_NVIDIA,
                    Some(8192),
                    Some(Date {
                        year: 2026,
                        month: 9,
                        day: 1,
                    }),
                ),
            ],
            power: Some(power_facts(PowerPlan::Balanced)),
            displays: vec![Display {
                name: "A".into(),
                current_hz: 60,
                max_hz: 60,
            }],
            background_recording: Some(false),
            game_mode: Some(true),
            windowed_optimizations: Some(true),
            hags: Some(true),
            memory_integrity: Some(true),
            memory: vec![stick(None, None), stick(None, None)],
            overlays: Some(vec![]),
            game_drive: Some(DriveKind::Ssd),
            ..Default::default()
        };
        let names: Vec<_> = checks(&f, TODAY).iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            [
                "GPU for Deadlock",
                "Power plan",
                "Refresh rate",
                "Background recording",
                "RAM",
                "Overlays",
                "GPU driver",
                "Game Mode",
                "Memory integrity",
                "Windowed game optimizations",
                "GPU scheduling",
                "Game drive",
            ]
        );
    }
}
