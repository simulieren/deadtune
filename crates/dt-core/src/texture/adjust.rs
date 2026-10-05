//! Colour adjustments for UI image overrides: a small ordered list of pure operations a
//! HUD build replays on the game's own pixels (or the player's PNG or SVG) every apply.
//! Textures go through [`apply_all`] on an [`RgbaImage`]; vector icons map every colour in
//! their text through [`adjust_rgb`] (`texture::svg::adjust`), so an SVG stays an SVG.
//!
//! Every operation but `Opacity` leaves alpha alone, and every colour operation but `Swap`
//! is a function of the pixel's own RGB, so the same arithmetic serves pixels and SVG fills.

use std::fmt;
use std::str::FromStr;

use super::png::RgbaImage;

/// An opaque colour, written as `#rrggbb` in profiles and levers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    pub const WHITE: Rgb = Rgb([255, 255, 255]);
    pub const BLACK: Rgb = Rgb([0, 0, 0]);

    pub fn hex(self) -> String {
        let [r, g, b] = self.0;
        format!("#{r:02x}{g:02x}{b:02x}")
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("colour {0:?} is not #rrggbb")]
pub struct RgbError(String);

impl FromStr for Rgb {
    type Err = RgbError;

    fn from_str(text: &str) -> Result<Rgb, RgbError> {
        let err = || RgbError(text.to_string());
        let hex = text.trim().strip_prefix('#').ok_or_else(err)?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| err());
        Ok(Rgb([channel(0)?, channel(2)?, channel(4)?]))
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

impl serde::Serialize for Rgb {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> serde::Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Rgb, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// How an overlay colour combines with the pixel under it.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
}

impl Blend {
    pub const ALL: [Blend; 4] = [
        Blend::Normal,
        Blend::Multiply,
        Blend::Screen,
        Blend::Overlay,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Blend::Normal => "normal",
            Blend::Multiply => "multiply",
            Blend::Screen => "screen",
            Blend::Overlay => "overlay",
        }
    }

    pub fn parse(text: &str) -> Option<Blend> {
        Blend::ALL.into_iter().find(|b| b.key() == text)
    }
}

/// One colour operation. Percentages are whole numbers; `clamped` keeps every field in its
/// range, and the setters below always store clamped values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Adjust {
    /// Multiplies by `color`, `strength` percent of the way (100 = fully multiplied).
    Tint {
        color: Rgb,
        strength: u8,
    },
    /// Takes `color`'s hue and saturation, keeps the pixel's luminance; recolours white and
    /// grey icons. `strength` percent of the way.
    Colorize {
        color: Rgb,
        strength: u8,
    },
    /// Lays `color` over the pixel with `blend`, at `opacity` percent.
    Overlay {
        color: Rgb,
        blend: Blend,
        opacity: u8,
    },
    /// Rotates hue by `degrees` (-180..=180).
    Hue {
        degrees: i16,
    },
    /// -100 is grey, 0 unchanged, 100 twice as saturated.
    Saturation {
        percent: i16,
    },
    /// Negative darkens towards black, positive lightens towards white.
    Brightness {
        percent: i16,
    },
    /// Negative flattens towards mid grey, positive stretches away from it.
    Contrast {
        percent: i16,
    },
    /// Scales alpha; the only operation that touches it.
    Opacity {
        percent: u8,
    },
    Invert,
    /// Replaces exactly `from` with `to` (an SVG palette swap; on a texture, exact pixels).
    Swap {
        from: Rgb,
        to: Rgb,
    },
}

/// Which slot of an override's list an adjustment fills: the colour operations (tint,
/// colorize, overlay) share one, each slider has its own, and each swapped colour its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AdjustKind {
    Color,
    Hue,
    Saturation,
    Brightness,
    Contrast,
    Opacity,
    Invert,
    Swap(Rgb),
}

impl Adjust {
    pub fn kind(&self) -> AdjustKind {
        match *self {
            Adjust::Tint { .. } | Adjust::Colorize { .. } | Adjust::Overlay { .. } => {
                AdjustKind::Color
            }
            Adjust::Hue { .. } => AdjustKind::Hue,
            Adjust::Saturation { .. } => AdjustKind::Saturation,
            Adjust::Brightness { .. } => AdjustKind::Brightness,
            Adjust::Contrast { .. } => AdjustKind::Contrast,
            Adjust::Opacity { .. } => AdjustKind::Opacity,
            Adjust::Invert => AdjustKind::Invert,
            Adjust::Swap { from, .. } => AdjustKind::Swap(from),
        }
    }

    /// The same operation with every field inside its range.
    pub fn clamped(self) -> Adjust {
        let pct = |p: u8| p.min(100);
        let signed = |p: i16| p.clamp(-100, 100);
        match self {
            Adjust::Tint { color, strength } => Adjust::Tint {
                color,
                strength: pct(strength),
            },
            Adjust::Colorize { color, strength } => Adjust::Colorize {
                color,
                strength: pct(strength),
            },
            Adjust::Overlay {
                color,
                blend,
                opacity,
            } => Adjust::Overlay {
                color,
                blend,
                opacity: pct(opacity),
            },
            Adjust::Hue { degrees } => Adjust::Hue {
                degrees: degrees.clamp(-180, 180),
            },
            Adjust::Saturation { percent } => Adjust::Saturation {
                percent: signed(percent),
            },
            Adjust::Brightness { percent } => Adjust::Brightness {
                percent: signed(percent),
            },
            Adjust::Contrast { percent } => Adjust::Contrast {
                percent: signed(percent),
            },
            Adjust::Opacity { percent } => Adjust::Opacity {
                percent: pct(percent),
            },
            Adjust::Invert | Adjust::Swap { .. } => self,
        }
    }

    /// An operation that changes nothing, which the setters drop from the list.
    pub fn is_identity(&self) -> bool {
        match *self {
            Adjust::Tint { strength, .. } | Adjust::Colorize { strength, .. } => strength == 0,
            Adjust::Overlay { opacity, .. } => opacity == 0,
            Adjust::Hue { degrees } => degrees == 0,
            Adjust::Saturation { percent }
            | Adjust::Brightness { percent }
            | Adjust::Contrast { percent } => percent == 0,
            Adjust::Opacity { percent } => percent == 100,
            Adjust::Invert => false,
            Adjust::Swap { from, to } => from == to,
        }
    }

    /// Plain words for the edits list: "Tint #ff0000 80%".
    pub fn label(&self) -> String {
        match *self {
            Adjust::Tint { color, strength } => format!("Tint {color} {strength}%"),
            Adjust::Colorize { color, strength } => format!("Colorize {color} {strength}%"),
            Adjust::Overlay {
                color,
                blend,
                opacity,
            } => format!("Overlay {color} {} {opacity}%", blend.key()),
            Adjust::Hue { degrees } => format!("Hue {degrees:+}"),
            Adjust::Saturation { percent } => format!("Saturation {percent:+}%"),
            Adjust::Brightness { percent } => format!("Brightness {percent:+}%"),
            Adjust::Contrast { percent } => format!("Contrast {percent:+}%"),
            Adjust::Opacity { percent } => format!("Opacity {percent}%"),
            Adjust::Invert => "Invert".into(),
            Adjust::Swap { from, to } => format!("Swap {from} to {to}"),
        }
    }

    /// Parses the lever and CLI spelling: `tint:#ff0000:80`, `colorize:#4d75c3:100`,
    /// `overlay:#000000:multiply:50`, `hue:30`, `saturation:-40`, `brightness:20`,
    /// `contrast:10`, `opacity:50`, `invert`, `swap:#ece8e1:#d4860b`.
    pub fn parse(spec: &str) -> Result<Adjust, String> {
        let parts: Vec<&str> = spec.split(':').map(str::trim).collect();
        let bad = || format!("{spec:?} is not an adjustment");
        let num = |text: &str| text.trim().parse::<i32>().map_err(|_| bad());
        let rgb = |text: &str| text.trim().parse::<Rgb>().map_err(|_| bad());
        let pct = |text: &str| num(text).map(|n| n.clamp(0, 100) as u8);
        let signed = |text: &str| num(text).map(|n| n.clamp(-100, 100) as i16);
        let adjust = match parts.as_slice() {
            ["tint", color, strength] => Adjust::Tint {
                color: rgb(color)?,
                strength: pct(strength)?,
            },
            ["colorize", color, strength] => Adjust::Colorize {
                color: rgb(color)?,
                strength: pct(strength)?,
            },
            ["overlay", color, blend, opacity] => Adjust::Overlay {
                color: rgb(color)?,
                blend: Blend::parse(blend.trim()).ok_or_else(bad)?,
                opacity: pct(opacity)?,
            },
            ["hue", degrees] => Adjust::Hue {
                degrees: num(degrees)?.clamp(-180, 180) as i16,
            },
            ["saturation", p] => Adjust::Saturation {
                percent: signed(p)?,
            },
            ["brightness", p] => Adjust::Brightness {
                percent: signed(p)?,
            },
            ["contrast", p] => Adjust::Contrast {
                percent: signed(p)?,
            },
            ["opacity", p] => Adjust::Opacity { percent: pct(p)? },
            ["invert"] => Adjust::Invert,
            ["swap", from, to] => Adjust::Swap {
                from: rgb(from)?,
                to: rgb(to)?,
            },
            _ => return Err(bad()),
        };
        Ok(adjust)
    }
}

impl fmt::Display for Adjust {
    /// The spelling `parse` reads.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Adjust::Tint { color, strength } => write!(f, "tint:{color}:{strength}"),
            Adjust::Colorize { color, strength } => write!(f, "colorize:{color}:{strength}"),
            Adjust::Overlay {
                color,
                blend,
                opacity,
            } => write!(f, "overlay:{color}:{}:{opacity}", blend.key()),
            Adjust::Hue { degrees } => write!(f, "hue:{degrees}"),
            Adjust::Saturation { percent } => write!(f, "saturation:{percent}"),
            Adjust::Brightness { percent } => write!(f, "brightness:{percent}"),
            Adjust::Contrast { percent } => write!(f, "contrast:{percent}"),
            Adjust::Opacity { percent } => write!(f, "opacity:{percent}"),
            Adjust::Invert => f.write_str("invert"),
            Adjust::Swap { from, to } => write!(f, "swap:{from}:{to}"),
        }
    }
}

/// Puts `adjust` in `list`: in place of the entry of the same kind when there is one, else
/// at the end. An identity operation removes that kind instead. Returns whether the list
/// changed.
pub fn set(list: &mut Vec<Adjust>, adjust: Adjust) -> bool {
    let adjust = adjust.clamped();
    if adjust.is_identity() {
        return remove(list, adjust.kind());
    }
    match list.iter_mut().find(|a| a.kind() == adjust.kind()) {
        Some(slot) if *slot == adjust => false,
        Some(slot) => {
            *slot = adjust;
            true
        }
        None => {
            list.push(adjust);
            true
        }
    }
}

pub fn remove(list: &mut Vec<Adjust>, kind: AdjustKind) -> bool {
    let before = list.len();
    list.retain(|a| a.kind() != kind);
    list.len() != before
}

pub fn apply_all(image: &mut RgbaImage, list: &[Adjust]) {
    for adjust in list {
        apply(image, adjust);
    }
}

mod ops;
pub use ops::{adjust_rgb, apply};

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb([255, 0, 0]);

    #[test]
    fn rgb_reads_and_writes_hex() {
        assert_eq!("#D4860B".parse::<Rgb>().unwrap(), Rgb([0xd4, 0x86, 0x0b]));
        assert_eq!(Rgb([0xd4, 0x86, 0x0b]).to_string(), "#d4860b");
        for bad in ["d4860b", "#d4860", "#d4860bff", "#gg0000", ""] {
            assert!(bad.parse::<Rgb>().is_err(), "{bad}");
        }
        let text = toml::to_string(&std::collections::BTreeMap::from([("c", RED)])).unwrap();
        assert_eq!(text.trim(), "c = \"#ff0000\"");
    }

    #[test]
    fn specs_round_trip_through_parse_and_display() {
        let all = [
            Adjust::Tint {
                color: RED,
                strength: 80,
            },
            Adjust::Colorize {
                color: Rgb([0x4d, 0x75, 0xc3]),
                strength: 100,
            },
            Adjust::Overlay {
                color: Rgb::BLACK,
                blend: Blend::Multiply,
                opacity: 50,
            },
            Adjust::Hue { degrees: -30 },
            Adjust::Saturation { percent: -40 },
            Adjust::Brightness { percent: 20 },
            Adjust::Contrast { percent: 10 },
            Adjust::Opacity { percent: 50 },
            Adjust::Invert,
            Adjust::Swap {
                from: Rgb::WHITE,
                to: RED,
            },
        ];
        for adjust in all {
            let text = adjust.to_string();
            assert_eq!(Adjust::parse(&text).unwrap(), adjust, "{text}");
        }
        assert_eq!(
            Adjust::parse(" hue : 400 ").unwrap(),
            Adjust::Hue { degrees: 180 },
            "clamped"
        );
        assert_eq!(
            Adjust::parse("tint:#ff0000:250").unwrap(),
            Adjust::Tint {
                color: RED,
                strength: 100
            }
        );
        for bad in [
            "tint",
            "tint:red:50",
            "hue:x",
            "overlay:#000000:soft:10",
            "",
            "swap:#000000",
        ] {
            assert!(Adjust::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn serde_is_tagged_by_kind_and_stable() {
        let list = vec![
            Adjust::Tint {
                color: RED,
                strength: 80,
            },
            Adjust::Overlay {
                color: Rgb::BLACK,
                blend: Blend::Screen,
                opacity: 30,
            },
            Adjust::Invert,
        ];
        let text = toml::to_string(&std::collections::BTreeMap::from([("adjust", &list)])).unwrap();
        assert!(
            text.contains("kind = \"tint\"") && text.contains("color = \"#ff0000\""),
            "{text}"
        );
        assert!(text.contains("blend = \"screen\""), "{text}");
        let back: std::collections::BTreeMap<String, Vec<Adjust>> = toml::from_str(&text).unwrap();
        assert_eq!(back["adjust"], list);
    }

    #[test]
    fn set_replaces_by_kind_and_drops_identities() {
        let mut list = Vec::new();
        assert!(set(
            &mut list,
            Adjust::Tint {
                color: RED,
                strength: 50
            }
        ));
        assert!(set(&mut list, Adjust::Hue { degrees: 30 }));
        assert!(set(
            &mut list,
            Adjust::Colorize {
                color: RED,
                strength: 90
            }
        ));
        assert_eq!(
            list,
            [
                Adjust::Colorize {
                    color: RED,
                    strength: 90
                },
                Adjust::Hue { degrees: 30 }
            ],
            "colorize took tint's slot, in place"
        );
        assert!(!set(&mut list, Adjust::Hue { degrees: 30 }), "same again");
        assert!(
            set(&mut list, Adjust::Hue { degrees: 0 }),
            "identity removes"
        );
        assert_eq!(list.len(), 1);
        assert!(!set(&mut list, Adjust::Opacity { percent: 100 }));
        assert!(set(&mut list, Adjust::Saturation { percent: -500 }));
        assert_eq!(list[1], Adjust::Saturation { percent: -100 }, "clamped");
        let swap = |from: Rgb| Adjust::Swap { from, to: RED };
        assert!(set(&mut list, swap(Rgb::WHITE)));
        assert!(set(&mut list, swap(Rgb::BLACK)));
        assert_eq!(list.len(), 4, "one slot per swapped colour");
        assert!(remove(&mut list, AdjustKind::Swap(Rgb::WHITE)));
        assert!(!remove(&mut list, AdjustKind::Swap(Rgb::WHITE)));
        assert!(remove(&mut list, AdjustKind::Color));
        assert_eq!(
            list,
            [Adjust::Saturation { percent: -100 }, swap(Rgb::BLACK)]
        );
    }

    #[test]
    fn labels_are_plain_words() {
        assert_eq!(
            Adjust::Tint {
                color: RED,
                strength: 80
            }
            .label(),
            "Tint #ff0000 80%"
        );
        assert_eq!(Adjust::Hue { degrees: -30 }.label(), "Hue -30");
        assert_eq!(
            Adjust::Brightness { percent: 20 }.label(),
            "Brightness +20%"
        );
    }
}
