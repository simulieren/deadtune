//! Apples and tunnels on the minimap: small dots on the fixed healing-apple spawn spots
//! on dl_midtown, and for the heroes that can travel the rat tunnels, dots on the tunnel
//! entrances near them. Optionally a clearer switch between the surface and tunnel maps.
//!
//! Idea by FesamAyt (GameBanana 724238 "Apple snacks and Tunnel Map", v0.3.1); rebuilt by
//! DeadTune from your game files. Only the idea and the factual spawn and entrance
//! positions come from that mod. Our script (`assets/apples_tunnels.js`) follows its
//! approach: read the local hero from the top bar's own portrait, place dots by percent
//! inside the minimap's background panel, and read tunnel view from the game's
//! `in_tunnels` class. Notes: research/hud/apples-tunnels/NOTES.md.
//!
//! The dots need our script, so they go in through `inject` (one stylesheet and one
//! script of ours, included in the game's own `hud_minimap.vxml_c`). Clear switching is
//! plain CSS appended to the game's `hud_minimap.vcss_c`, so on its own it rebuilds no
//! layout.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use super::css::emit_rule;
use super::inject::LayoutEdit;
use super::minimap_colors::Color;

pub const MINIMAP_LAYOUT: &str = "panorama/layout/hud_minimap.vxml_c";
pub const OWN_SCRIPT: &str = "panorama/scripts/deadtune/apples_tunnels.vjs_c";
pub const OWN_STYLE: &str = "panorama/styles/deadtune/apples_tunnels.vcss_c";

const SCRIPT: &str = include_str!("assets/apples_tunnels.js");

/// The Deadlock build the positions below were taken on.
pub const GAME_BUILD: u32 = 25_712_201;
/// The dl_midtown map build of `GAME_BUILD`.
pub const MAP_BUILD: u32 = 6722;

pub const DOT_SIZE_RANGE: RangeInclusive<u8> = 3..=12;
/// Percent of the map width.
pub const RADIUS_RANGE: RangeInclusive<u8> = 5..=25;
/// An entrance stays shown until the hero is this many percent past the show radius.
pub const HIDE_MARGIN_PCT: u8 = 2;

/// A spot on the minimap picture as fractions of its size: `u` left to right, `v` top to
/// bottom, both 0..=1. Panorama places our dots with `position: u% v%` inside
/// `#MinimapBackgroundTest`, so these are the numbers the script uses as they are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapPoint {
    pub u: f64,
    pub v: f64,
}

impl MapPoint {
    const fn new(u: f64, v: f64) -> MapPoint {
        MapPoint { u, v }
    }
}

// Valid for GAME_BUILD / MAP_BUILD. Valve moves apples and entrances now and then; when
// the dots drift off their spots in game, refresh both tables (and the two builds) with
// research/hud/apples-tunnels/extract_points.py or by measuring in the hideout.

/// The 36 healing-apple spawn spots. Dots mark where to look, not whether an apple is
/// there now.
pub static APPLES: [MapPoint; 36] = [
    MapPoint::new(0.76464844, 0.51655506),
    MapPoint::new(0.22991071, 0.47991071),
    MapPoint::new(0.58333333, 0.55357143),
    MapPoint::new(0.69940476, 0.64583333),
    MapPoint::new(0.58779762, 0.70238095),
    MapPoint::new(0.6860119, 0.70386905),
    MapPoint::new(0.64583333, 0.68005952),
    MapPoint::new(0.86941964, 0.63764881),
    MapPoint::new(0.21279762, 0.578125),
    MapPoint::new(0.23809524, 0.64360119),
    MapPoint::new(0.32811306, 0.65922619),
    MapPoint::new(0.27232143, 0.55059524),
    MapPoint::new(0.27008929, 0.67261905),
    MapPoint::new(0.65029762, 0.44494048),
    MapPoint::new(0.73363095, 0.44233631),
    MapPoint::new(0.74404762, 0.33779762),
    MapPoint::new(0.74404762, 0.31696429),
    MapPoint::new(0.42261905, 0.44047618),
    MapPoint::new(0.24702381, 0.37743925),
    MapPoint::new(0.30803571, 0.70684524),
    MapPoint::new(0.20982143, 0.66517857),
    MapPoint::new(0.21875, 0.67113095),
    MapPoint::new(0.17261905, 0.29464283),
    MapPoint::new(0.10119048, 0.64583333),
    MapPoint::new(0.38392857, 0.26785714),
    MapPoint::new(0.13364955, 0.3139881),
    MapPoint::new(0.13048735, 0.3139881),
    MapPoint::new(0.12723214, 0.38095238),
    MapPoint::new(0.77557398, 0.30952381),
    MapPoint::new(0.69494048, 0.29166667),
    MapPoint::new(0.62797619, 0.26785714),
    MapPoint::new(0.85416667, 0.30654762),
    MapPoint::new(0.89583333, 0.35866748),
    MapPoint::new(0.61904762, 0.73809524),
    MapPoint::new(0.83928571, 0.73214286),
    MapPoint::new(0.76190476, 0.78869048),
];

/// The 24 rat tunnel entrances, from the map's own entrance entities.
pub static TUNNEL_ENTRANCES: [MapPoint; 24] = [
    MapPoint::new(0.64136905, 0.55357143),
    MapPoint::new(0.55877976, 0.51785714),
    MapPoint::new(0.47470238, 0.55505952),
    MapPoint::new(0.43824405, 0.52083333),
    MapPoint::new(0.32738095, 0.58705357),
    MapPoint::new(0.171875, 0.60714286),
    MapPoint::new(0.42782738, 0.60342262),
    MapPoint::new(0.27566964, 0.69494048),
    MapPoint::new(0.53348215, 0.66071429),
    MapPoint::new(0.70825779, 0.57427646),
    MapPoint::new(0.734375, 0.63839286),
    MapPoint::new(0.63746165, 0.55956503),
    MapPoint::new(0.4360119, 0.56547619),
    MapPoint::new(0.36210101, 0.44646717),
    MapPoint::new(0.45014881, 0.48214286),
    MapPoint::new(0.52380952, 0.44940476),
    MapPoint::new(0.67782738, 0.44159226),
    MapPoint::new(0.56175595, 0.48102679),
    MapPoint::new(0.2916988, 0.43243866),
    MapPoint::new(0.26450893, 0.36197917),
    MapPoint::new(0.56063988, 0.43445635),
    MapPoint::new(0.36755952, 0.44047619),
    MapPoint::new(0.57217263, 0.39657739),
    MapPoint::new(0.46651787, 0.33928571),
];

/// Heroes that can enter the rat tunnels. Calico counts in either form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelHero {
    Rem,
    MoAndKrill,
    RatKing,
    Calico,
}

impl TunnelHero {
    pub const ALL: [TunnelHero; 4] = [
        TunnelHero::Rem,
        TunnelHero::MoAndKrill,
        TunnelHero::RatKing,
        TunnelHero::Calico,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TunnelHero::Rem => "Rem",
            TunnelHero::MoAndKrill => "Mo & Krill",
            TunnelHero::RatKing => "Rat King",
            TunnelHero::Calico => "Calico",
        }
    }

    /// The game's localisation token, so the script also matches non-English clients.
    pub fn token(self) -> &'static str {
        match self {
            TunnelHero::Rem => "#hero_familiar",
            TunnelHero::MoAndKrill => "#hero_krill",
            TunnelHero::RatKing => "#hero_ratking",
            TunnelHero::Calico => "#hero_nano",
        }
    }
}

/// One kind of dot: shown or not, its size in px at 1080p and its colour. A table in a
/// profile states all three; each kind has its own defaults on `ApplesTunnels`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Dots {
    pub on: bool,
    pub size_px: u8,
    pub color: Color,
}

/// `Default` is vanilla: nothing emitted, no addon files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ApplesTunnels {
    /// Every hero sees these, in tunnel view too.
    pub apples: Dots,
    /// Only for `TunnelHero`es, only near the hero, hidden in tunnel view.
    pub tunnels: Dots,
    /// Entrances closer than this percent of the map width are shown.
    pub tunnel_radius_pct: u8,
    /// The surface map fades to a faint outline and the tunnel layer brightens in tunnel view.
    pub clear_switching: bool,
}

pub const APPLE_COLOR: Color = Color([0x74, 0xF0, 0x6A, 255]);
pub const TUNNEL_COLOR: Color = Color([0xE5, 0xB8, 0xFF, 255]);

impl Default for ApplesTunnels {
    fn default() -> Self {
        ApplesTunnels {
            apples: Dots {
                on: false,
                size_px: 5,
                color: APPLE_COLOR,
            },
            tunnels: Dots {
                on: false,
                size_px: 5,
                color: TUNNEL_COLOR,
            },
            tunnel_radius_pct: 11,
            clear_switching: false,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ApplesTunnelsError {
    #[error("dot size {0}px outside 3..=12")]
    DotSize(u8),
    #[error("tunnel radius {0}% outside 5..=25")]
    Radius(u8),
}

/// What apples and tunnels add to the addon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplesTunnelsPatch {
    /// Appended to the game's `hud_minimap.vcss_c`.
    pub css: String,
    /// The minimap layout's additions, when a dot kind is on.
    pub layout: Option<LayoutEdit>,
    /// Our script and stylesheet, when a dot kind is on.
    pub own_files: BTreeMap<String, String>,
}

/// Dots sit in `#MinimapBackgroundTest`, whose ancestors carry the game's `dl_midtown`
/// and `in_tunnels` classes (vanilla `hud_minimap.css` keys its map layers on the pair).
const MAP: &str = ".dl_midtown #MinimapBackgroundTest";
const TUNNEL_VIEW: &str = ".dl_midtown.in_tunnels #MinimapBackgroundTest";

impl ApplesTunnels {
    pub fn is_vanilla(&self) -> bool {
        *self == ApplesTunnels::default()
    }

    pub fn has_dots(&self) -> bool {
        self.apples.on || self.tunnels.on
    }

    /// Switches on, for the page's "Changed" count.
    pub fn changed_count(&self) -> usize {
        usize::from(self.apples.on)
            + usize::from(self.tunnels.on)
            + usize::from(self.clear_switching)
    }

    pub fn validate(&self) -> Result<(), ApplesTunnelsError> {
        for dots in [self.apples, self.tunnels] {
            if !DOT_SIZE_RANGE.contains(&dots.size_px) {
                return Err(ApplesTunnelsError::DotSize(dots.size_px));
            }
        }
        if !RADIUS_RANGE.contains(&self.tunnel_radius_pct) {
            return Err(ApplesTunnelsError::Radius(self.tunnel_radius_pct));
        }
        Ok(())
    }

    /// Rules appended to the game's minimap stylesheet: the clearer layer switch.
    pub fn switching_css(&self) -> String {
        if !self.clear_switching {
            return String::new();
        }
        let layers = (1..=3)
            .map(|n| format!("{MAP} .backgroundImage{n}"))
            .collect::<Vec<_>>()
            .join(",");
        [
            emit_rule(
                &layers,
                &[
                    ("transition-property", "opacity".into()),
                    ("transition-duration", "0.18s".into()),
                ],
            ),
            emit_rule(
                &format!("{TUNNEL_VIEW} .backgroundImage1"),
                &[("opacity", "0.15".into())],
            ),
            emit_rule(
                &format!("{TUNNEL_VIEW} .backgroundImage2"),
                &[("opacity", "0".into())],
            ),
            emit_rule(
                &format!("{TUNNEL_VIEW} .backgroundImage3"),
                &[("opacity", "1".into()), ("brightness", "1.15".into())],
            ),
        ]
        .concat()
    }

    /// Our stylesheet for the dots that are on. Uses no game `@define`s.
    pub fn dots_css(&self) -> String {
        let mut out = String::new();
        let kinds = [
            (self.apples, "#DtApples", "DtAppleDot", "1"),
            (self.tunnels, "#DtTunnelEntrances", "DtTunnelDot", "2"),
        ];
        for (dots, layer, class, z) in kinds.into_iter().filter(|(d, ..)| d.on) {
            out.push_str(&emit_rule(
                layer,
                &[
                    ("width", "100%".into()),
                    ("height", "100%".into()),
                    ("z-index", z.into()),
                    ("visibility", "collapse".into()),
                ],
            ));
            out.push_str(&emit_rule(
                &format!(".dl_midtown {layer}"),
                &[("visibility", "visible".into())],
            ));
            let size = dots.size_px;
            let half = number(f64::from(size) / 2.0);
            let mut decls = vec![
                ("width", format!("{size}px")),
                ("height", format!("{size}px")),
                (
                    "transform",
                    format!("translateX(-{half}px) translateY(-{half}px)"),
                ),
                ("background-color", dots.color.to_string()),
                ("border", "1px solid #000000B0".into()),
                ("border-radius", "50%".into()),
            ];
            if class == "DtTunnelDot" {
                decls.push(("visibility", "collapse".into()));
            }
            out.push_str(&emit_rule(&format!(".{class}"), &decls));
        }
        if self.tunnels.on {
            out.push_str(&emit_rule(
                ".DtTunnelDot.DtNear",
                &[("visibility", "visible".into())],
            ));
            out.push_str(&emit_rule(
                ".dl_midtown.in_tunnels #DtTunnelEntrances",
                &[("visibility", "collapse".into())],
            ));
        }
        out
    }

    /// The script with this style's tables, or `None` when no dots are on. The positions
    /// and hero names come from the tables above, so the script carries no map data of
    /// its own.
    pub fn script(&self) -> Option<String> {
        if !self.has_dots() {
            return None;
        }
        let points = |on: bool, table: &[MapPoint]| {
            if on {
                serde_json::Value::from(
                    table
                        .iter()
                        .map(|p| vec![p.u, p.v])
                        .collect::<Vec<Vec<f64>>>(),
                )
            } else {
                serde_json::Value::Null
            }
        };
        let show = f64::from(self.tunnel_radius_pct) / 100.0;
        let hide = f64::from(self.tunnel_radius_pct + HIDE_MARGIN_PCT) / 100.0;
        let config = serde_json::json!({
            "apples": points(self.apples.on, &APPLES),
            "tunnels": points(self.tunnels.on, &TUNNEL_ENTRANCES),
            "show": show,
            "hide": hide,
            "heroes": TunnelHero::ALL.map(TunnelHero::name),
            "tokens": TunnelHero::ALL.map(TunnelHero::token),
        });
        Some(format!("var DT_MAP = {config};\n{SCRIPT}"))
    }

    pub fn compile(&self) -> Result<ApplesTunnelsPatch, ApplesTunnelsError> {
        self.validate()?;
        let mut patch = ApplesTunnelsPatch {
            css: self.switching_css(),
            ..ApplesTunnelsPatch::default()
        };
        if let Some(script) = self.script() {
            patch.layout = Some(LayoutEdit {
                style_includes: vec![format!("s2r://{OWN_STYLE}")],
                script_includes: vec![format!("s2r://{OWN_SCRIPT}")],
                panels: Vec::new(),
            });
            patch.own_files.insert(OWN_SCRIPT.to_string(), script);
            patch
                .own_files
                .insert(OWN_STYLE.to_string(), self.dots_css());
        }
        Ok(patch)
    }
}

fn number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{css, inject};

    const VANILLA_LAYOUT: &[u8] =
        include_bytes!("../../tests/fixtures/hud/hud_minimap_vanilla.vxml_c");

    fn on() -> ApplesTunnels {
        let mut s = ApplesTunnels::default();
        s.apples.on = true;
        s.tunnels.on = true;
        s
    }

    #[test]
    fn tables_hold_36_apples_and_24_entrances_inside_the_map() {
        assert_eq!(APPLES.len(), 36);
        assert_eq!(TUNNEL_ENTRANCES.len(), 24);
        for p in APPLES.iter().chain(&TUNNEL_ENTRANCES) {
            assert!(
                (0.0..=1.0).contains(&p.u) && (0.0..=1.0).contains(&p.v),
                "{p:?}"
            );
            assert!(
                (0.05..=0.95).contains(&p.u) && (0.05..=0.95).contains(&p.v),
                "{p:?} sits on the map, not its border"
            );
        }
        for table in [&APPLES[..], &TUNNEL_ENTRANCES[..]] {
            for (i, a) in table.iter().enumerate() {
                for b in &table[i + 1..] {
                    assert!(
                        (a.u - b.u).abs() + (a.v - b.v).abs() > 0.001,
                        "duplicate spot {a:?}"
                    );
                }
            }
        }
        assert_eq!((GAME_BUILD, MAP_BUILD), (25_712_201, 6722));
    }

    #[test]
    fn vanilla_writes_nothing() {
        let s = ApplesTunnels::default();
        assert!(s.is_vanilla());
        assert_eq!(s.changed_count(), 0);
        assert_eq!(s.compile().unwrap(), ApplesTunnelsPatch::default());
        let mut recoloured = s;
        recoloured.apples.color = Color([255, 0, 0, 255]);
        recoloured.tunnels.size_px = 9;
        assert_eq!(
            recoloured.compile().unwrap(),
            ApplesTunnelsPatch::default(),
            "options of switched-off dots write nothing"
        );
    }

    #[test]
    fn clear_switching_alone_is_css_on_the_game_stylesheet() {
        let s = ApplesTunnels {
            clear_switching: true,
            ..ApplesTunnels::default()
        };
        let patch = s.compile().unwrap();
        assert!(patch.layout.is_none() && patch.own_files.is_empty());
        assert_eq!(
            patch.css,
            ".dl_midtown #MinimapBackgroundTest .backgroundImage1,\
             .dl_midtown #MinimapBackgroundTest .backgroundImage2,\
             .dl_midtown #MinimapBackgroundTest .backgroundImage3\
             {transition-property:opacity;transition-duration:0.18s;}\
             .dl_midtown.in_tunnels #MinimapBackgroundTest .backgroundImage1{opacity:0.15;}\
             .dl_midtown.in_tunnels #MinimapBackgroundTest .backgroundImage2{opacity:0;}\
             .dl_midtown.in_tunnels #MinimapBackgroundTest .backgroundImage3{opacity:1;brightness:1.15;}"
        );
        assert_eq!(css::parse_rules(&patch.css).unwrap().len(), 4);
    }

    #[test]
    fn apples_alone_add_our_files_without_tunnel_rules() {
        let mut s = ApplesTunnels::default();
        s.apples.on = true;
        let patch = s.compile().unwrap();
        assert_eq!(patch.css, "");
        let edit = patch.layout.expect("layout edit");
        assert_eq!(
            edit.style_includes,
            ["s2r://panorama/styles/deadtune/apples_tunnels.vcss_c"]
        );
        assert_eq!(
            edit.script_includes,
            ["s2r://panorama/scripts/deadtune/apples_tunnels.vjs_c"]
        );
        let sheet = &patch.own_files[OWN_STYLE];
        assert_eq!(
            sheet,
            "#DtApples{width:100%;height:100%;z-index:1;visibility:collapse;}\
             .dl_midtown #DtApples{visibility:visible;}\
             .DtAppleDot{width:5px;height:5px;transform:translateX(-2.5px) translateY(-2.5px);\
             background-color:#74F06A;border:1px solid #000000B0;border-radius:50%;}"
        );
        let script = &patch.own_files[OWN_SCRIPT];
        let config = script.lines().next().unwrap();
        assert!(config.starts_with("var DT_MAP = {"), "{config}");
        assert!(config.contains("\"tunnels\":null"), "{config}");
        assert!(config.contains("[0.76464844,0.51655506]"), "{config}");
        assert_eq!(config.matches("],[").count(), 35);
    }

    #[test]
    fn tunnels_carry_size_colour_radius_and_heroes() {
        let s = ApplesTunnels {
            tunnels: Dots {
                on: true,
                size_px: 8,
                color: Color([0xAA, 0x55, 0xFF, 0xC0]),
            },
            tunnel_radius_pct: 15,
            ..ApplesTunnels::default()
        };
        let patch = s.compile().unwrap();
        let sheet = &patch.own_files[OWN_STYLE];
        assert!(!sheet.contains("DtApple"), "{sheet}");
        assert!(sheet.contains(
            ".DtTunnelDot{width:8px;height:8px;transform:translateX(-4px) translateY(-4px);\
             background-color:#AA55FFC0;"
        ));
        assert!(sheet.ends_with(
            ".DtTunnelDot.DtNear{visibility:visible;}\
             .dl_midtown.in_tunnels #DtTunnelEntrances{visibility:collapse;}"
        ));
        assert!(css::parse_rules(sheet).unwrap().len() >= 5);
        let script = &patch.own_files[OWN_SCRIPT];
        let config = script.lines().next().unwrap();
        for want in [
            "\"apples\":null",
            "\"show\":0.15",
            "\"hide\":0.17",
            "\"heroes\":[\"Rem\",\"Mo & Krill\",\"Rat King\",\"Calico\"]",
            "\"tokens\":[\"#hero_familiar\",\"#hero_krill\",\"#hero_ratking\",\"#hero_nano\"]",
            "[0.64136905,0.55357143]",
        ] {
            assert!(config.contains(want), "{want} in {config}");
        }
        assert!(script.ends_with("$.Schedule(1.0, tick);\n})();\n"));
    }

    #[test]
    fn script_uses_only_its_config_for_map_data() {
        assert!(!SCRIPT.contains("0.64136905"));
        assert!(!SCRIPT.contains("Nathan"));
        assert!(SCRIPT.contains("CONFIG = DT_MAP"));
        assert_eq!(ApplesTunnels::default().script(), None);
    }

    #[test]
    fn layout_decodes_back_with_our_includes_only_added() {
        let patch = on().compile().unwrap();
        let edit = patch.layout.unwrap();
        let bytes = inject::patched_layout(VANILLA_LAYOUT, &edit, &[], "test").unwrap();
        let rebuilt = inject::layout_text(&bytes).unwrap();
        let original = inject::layout_text(VANILLA_LAYOUT).unwrap();
        assert!(inject::extends(&rebuilt, &original));
        assert!(rebuilt.contains(
            "\t\t<include src=\"s2r://panorama/styles/deadtune/apples_tunnels.vcss_c\" />\n\t</styles>\n\t<scripts>\n\t\t<include src=\"s2r://panorama/scripts/deadtune/apples_tunnels.vjs_c\" />\n\t</scripts>\n"
        ), "{rebuilt}");
        assert!(rebuilt.contains("id=\"MinimapBackgroundTest\""));
        assert_eq!(rebuilt.lines().count(), original.lines().count() + 5);
    }

    #[test]
    fn out_of_range_values_are_refused() {
        let mut s = on();
        s.apples.size_px = 2;
        assert_eq!(s.compile(), Err(ApplesTunnelsError::DotSize(2)));
        let mut s = on();
        s.tunnels.size_px = 13;
        assert_eq!(s.compile(), Err(ApplesTunnelsError::DotSize(13)));
        let mut s = on();
        s.tunnel_radius_pct = 30;
        assert_eq!(s.compile(), Err(ApplesTunnelsError::Radius(30)));
    }

    #[test]
    fn toml_round_trip_and_partial_tables() {
        let mut s = on();
        s.tunnels.color = Color([1, 2, 3, 255]);
        s.clear_switching = true;
        let text = toml::to_string(&s).unwrap();
        assert!(text.contains("[tunnels]"), "{text}");
        assert!(text.contains("color = \"#010203\""), "{text}");
        assert_eq!(toml::from_str::<ApplesTunnels>(&text).unwrap(), s);
        let partial: ApplesTunnels = toml::from_str("clear_switching = true\n").unwrap();
        assert_eq!(
            partial,
            ApplesTunnels {
                clear_switching: true,
                ..ApplesTunnels::default()
            }
        );
    }
}
