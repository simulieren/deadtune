//! Pure parsers for the system reader: power GUIDs, `key=value;` strings, the RAM
//! JSON from PowerShell, overlay matching and mount selection.

use crate::winfps::facts::{MemoryStick, OVERLAYS, PowerMode, PowerPlan};

/// Canonical lowercase `8-4-4-4-12` form of a GUID's fields.
pub fn guid_string(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> String {
    format!(
        "{data1:08x}-{data2:04x}-{data3:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        data4[0], data4[1], data4[2], data4[3], data4[4], data4[5], data4[6], data4[7]
    )
}

/// Built-in plan for a GUID string; `None` for vendor and user plans.
pub fn power_plan(guid: &str) -> Option<PowerPlan> {
    match guid
        .trim()
        .trim_matches(['{', '}'])
        .to_ascii_lowercase()
        .as_str()
    {
        "a1841308-3541-4fab-bc81-f71556f20b4a" => Some(PowerPlan::PowerSaver),
        "381b4222-f694-41f0-9685-ff5bb260df2e" => Some(PowerPlan::Balanced),
        "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c" => Some(PowerPlan::HighPerformance),
        "e9a42b02-d5df-448d-aa00-03f14749eb61" => Some(PowerPlan::UltimatePerformance),
        _ => None,
    }
}

/// Windows 11 power mode overlay; unknown GUIDs give `None`.
pub fn power_mode(guid: &str) -> Option<PowerMode> {
    match guid
        .trim()
        .trim_matches(['{', '}'])
        .to_ascii_lowercase()
        .as_str()
    {
        "ded574b5-45a0-4f42-8737-46345c09c238" => Some(PowerMode::BestPerformance),
        "961cc777-2547-4f9d-8174-7d86181b8a7a" => Some(PowerMode::BestPowerEfficiency),
        "00000000-0000-0000-0000-000000000000" => Some(PowerMode::Balanced),
        _ => None,
    }
}

/// Value of `key` in a `k=v;k=v;` string (key compared case-insensitively).
pub fn pair_value<'a>(settings: &'a str, key: &str) -> Option<&'a str> {
    settings.split(';').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        k.trim().eq_ignore_ascii_case(key).then_some(v.trim())
    })
}

/// `SwapEffectUpgradeEnable` from `DirectXUserGlobalSettings`.
pub fn swap_effect_upgrade(settings: &str) -> Option<bool> {
    match pair_value(settings, "SwapEffectUpgradeEnable")? {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// Display names of the known overlays among `process_names`, in `OVERLAYS` order.
pub fn match_overlays<S: AsRef<str>>(process_names: &[S]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for (exe, display) in OVERLAYS {
        let running = process_names
            .iter()
            .any(|n| n.as_ref().eq_ignore_ascii_case(exe));
        if running && !out.contains(display) {
            out.push(display);
        }
    }
    out
}

fn normalize_path(p: &str) -> String {
    let p = p.strip_prefix(r"\\?\").unwrap_or(p);
    let mut s = p.replace('/', "\\").to_lowercase();
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

/// Index of the mount point that is the longest path-component prefix of `path`.
pub fn longest_mount<S: AsRef<str>>(path: &str, mounts: &[S]) -> Option<usize> {
    let path = normalize_path(path);
    let mut best: Option<(usize, usize)> = None;
    for (i, mount) in mounts.iter().enumerate() {
        let m = normalize_path(mount.as_ref());
        let covers = path == m
            || path
                .strip_prefix(&m)
                .is_some_and(|rest| rest.starts_with('\\'));
        if covers && best.is_none_or(|(_, len)| m.len() > len) {
            best = Some((i, m.len()));
        }
    }
    best.map(|(i, _)| i)
}

/// RAM sticks from `Get-CimInstance Win32_PhysicalMemory | ConvertTo-Json -Compress`.
/// Empty on anything unexpected.
pub fn memory_sticks(json: &str) -> Vec<MemoryStick> {
    let Some(objects) = parse_objects(json) else {
        return Vec::new();
    };
    let mut sticks = Vec::new();
    for obj in objects {
        let get = |name: &str| obj.iter().find(|(k, _)| k == name).map(|(_, v)| v);
        let Some(Value::Num(bytes)) = get("Capacity") else {
            return Vec::new();
        };
        if *bytes < 0.0 {
            return Vec::new();
        }
        let mts = |name: &str| match get(name) {
            Some(Value::Num(n)) if *n > 0.0 && *n <= f64::from(u32::MAX) => Some(*n as u32),
            _ => None,
        };
        sticks.push(MemoryStick {
            capacity_mib: (*bytes / 1_048_576.0).round() as u64,
            rated_mts: mts("Speed"),
            configured_mts: mts("ConfiguredClockSpeed"),
        });
    }
    sticks
}

#[derive(Debug)]
enum Value {
    Num(f64),
    Other,
}

type Object = Vec<(String, Value)>;

/// A single flat object or an array of them.
fn parse_objects(json: &str) -> Option<Vec<Object>> {
    let mut p = Parser {
        s: json.trim_start_matches('\u{feff}').as_bytes(),
        i: 0,
    };
    p.ws();
    let out = match p.peek()? {
        b'{' => vec![p.object()?],
        b'[' => {
            p.i += 1;
            let mut v = Vec::new();
            p.ws();
            if p.peek()? == b']' {
                p.i += 1;
            } else {
                loop {
                    p.ws();
                    v.push(p.object()?);
                    p.ws();
                    match p.next()? {
                        b',' => {}
                        b']' => break,
                        _ => return None,
                    }
                }
            }
            v
        }
        _ => return None,
    };
    p.ws();
    (p.i == p.s.len()).then_some(out)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }

    fn ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn object(&mut self) -> Option<Object> {
        if self.next()? != b'{' {
            return None;
        }
        let mut obj = Vec::new();
        self.ws();
        if self.peek()? == b'}' {
            self.i += 1;
            return Some(obj);
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            if self.next()? != b':' {
                return None;
            }
            self.ws();
            let value = self.value()?;
            obj.push((key, value));
            self.ws();
            match self.next()? {
                b',' => {}
                b'}' => return Some(obj),
                _ => return None,
            }
        }
    }

    fn value(&mut self) -> Option<Value> {
        match self.peek()? {
            b'"' => self.string().map(|_| Value::Other),
            b'-' | b'0'..=b'9' => {
                let start = self.i;
                while self
                    .peek()
                    .is_some_and(|c| matches!(c, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
                {
                    self.i += 1;
                }
                let text = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                text.parse().ok().map(Value::Num)
            }
            _ => {
                for lit in ["null", "true", "false"] {
                    if self.s[self.i..].starts_with(lit.as_bytes()) {
                        self.i += lit.len();
                        return Some(Value::Other);
                    }
                }
                None
            }
        }
    }

    /// A string; escapes are skipped over (keys here are plain ASCII).
    fn string(&mut self) -> Option<String> {
        if self.next()? != b'"' {
            return None;
        }
        let start = self.i;
        loop {
            match self.next()? {
                b'"' => break,
                b'\\' => {
                    self.next()?;
                }
                _ => {}
            }
        }
        Some(String::from_utf8_lossy(&self.s[start..self.i - 1]).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans() {
        assert_eq!(
            power_plan("{381B4222-F694-41F0-9685-FF5BB260DF2E}"),
            Some(PowerPlan::Balanced)
        );
        assert_eq!(
            power_plan("e9a42b02-d5df-448d-aa00-03f14749eb61"),
            Some(PowerPlan::UltimatePerformance)
        );
        assert_eq!(
            power_plan("a1841308-3541-4fab-bc81-f71556f20b4a"),
            Some(PowerPlan::PowerSaver)
        );
        assert_eq!(
            power_plan("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"),
            Some(PowerPlan::HighPerformance)
        );
        assert_eq!(power_plan("12345678-0000-0000-0000-000000000000"), None);
    }

    #[test]
    fn guid_formatting() {
        let s = guid_string(
            0x381b4222,
            0xf694,
            0x41f0,
            [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e],
        );
        assert_eq!(s, "381b4222-f694-41f0-9685-ff5bb260df2e");
    }

    #[test]
    fn modes() {
        assert_eq!(
            power_mode("ded574b5-45a0-4f42-8737-46345c09c238"),
            Some(PowerMode::BestPerformance)
        );
        assert_eq!(
            power_mode("961CC777-2547-4F9D-8174-7D86181B8A7A"),
            Some(PowerMode::BestPowerEfficiency)
        );
        assert_eq!(
            power_mode("00000000-0000-0000-0000-000000000000"),
            Some(PowerMode::Balanced)
        );
        assert_eq!(power_mode("3af9b8d9-7c97-431d-ad78-34a8bfea439f"), None);
        assert_eq!(power_mode(""), None);
    }

    #[test]
    fn pairs() {
        let s = "SwapEffectUpgradeEnable=1;VRROptimizeEnable=0;";
        assert_eq!(pair_value(s, "VRROptimizeEnable"), Some("0"));
        assert_eq!(pair_value(s, "Missing"), None);
        assert_eq!(swap_effect_upgrade(s), Some(true));
        assert_eq!(
            swap_effect_upgrade("VRROptimizeEnable=0;SwapEffectUpgradeEnable=0"),
            Some(false)
        );
        assert_eq!(swap_effect_upgrade("VRROptimizeEnable=0;"), None);
        assert_eq!(swap_effect_upgrade("SwapEffectUpgradeEnable=x;"), None);
        assert_eq!(swap_effect_upgrade(""), None);
    }

    #[test]
    fn overlay_matching() {
        let names = ["explorer.exe", "OBS64.EXE", "discord.exe", "Discord.exe"];
        assert_eq!(match_overlays(&names), vec!["Discord", "OBS Studio"]);
        assert!(match_overlays::<&str>(&[]).is_empty());
    }

    #[test]
    fn mounts() {
        let mounts = ["C:\\", "D:\\", "D:\\Games"];
        assert_eq!(longest_mount(r"d:\games\steam\Deadlock", &mounts), Some(2));
        assert_eq!(longest_mount(r"D:\Gamesx\a", &mounts), Some(1));
        assert_eq!(
            longest_mount(r"\\?\C:\Program Files\Steam", &mounts),
            Some(0)
        );
        assert_eq!(longest_mount(r"E:\x", &mounts), None);
        assert_eq!(longest_mount("D:/Games", &mounts), Some(2));
    }

    #[test]
    fn memory_array_and_object() {
        let two = r#"[{"Capacity":17179869184,"Speed":3600,"ConfiguredClockSpeed":3200},{"Capacity":17179869184,"Speed":3600,"ConfiguredClockSpeed":null}]"#;
        let sticks = memory_sticks(two);
        assert_eq!(sticks.len(), 2);
        assert_eq!(
            sticks[0],
            MemoryStick {
                capacity_mib: 16384,
                rated_mts: Some(3600),
                configured_mts: Some(3200)
            }
        );
        assert_eq!(sticks[1].configured_mts, None);
        let one = "{\"Capacity\": 8589934592, \"Speed\": null, \"ConfiguredClockSpeed\": 2133}\r\n";
        assert_eq!(
            memory_sticks(one),
            vec![MemoryStick {
                capacity_mib: 8192,
                rated_mts: None,
                configured_mts: Some(2133)
            }]
        );
    }

    #[test]
    fn memory_rejects_bad_input() {
        for bad in [
            "",
            "null",
            "[]x",
            "{\"Capacity\":",
            "[{\"Speed\":3200}]",
            "{\"Capacity\":\"8\"}",
            "[{\"Capacity\":1},]",
            "garbage",
        ] {
            assert!(memory_sticks(bad).is_empty(), "{bad}");
        }
        assert!(memory_sticks("[]").is_empty());
    }
}
