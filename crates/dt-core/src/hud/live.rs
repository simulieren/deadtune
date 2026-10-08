//! Live HUD preview: restyles the running game's HUD without a restart.
//!
//! The running game locks the HUD pak and Panorama has no reload, so the pak ships one
//! script of ours (`assets/live_hud.js`) that sets `panel.style.<prop>` from rules DeadTune
//! hands it. Panorama JS cannot read a ConVar, but a `CitadelSettingsSlider` bound to one
//! shows its value, so the pak also carries hidden sliders (`SLOTS`) on harmless numeric
//! ConVars: a control slot (sequence and chunk number) and data slots (16-bit words). DeadTune
//! sets them through the console (netcon, or the bound key running `exec deadtune_hud`); the
//! script reads the sliders four times a second, which prints nothing. Every message carries
//! the `base` of the pak the game runs, so overrides are always relative to what the pak
//! baked. Design: docs/plans/live-hud/plan.md.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::css;
use super::elements::ELEMENTS;
use super::inject::{Anchor, Element, LayoutEdit};
use super::layout::{self, HudFeature, HudLayout, HudPatch, LayoutError};
use crate::bridge::{BridgeError, ConsoleCmd, execfile};

pub const LIVE: &str = "DEADTUNE_LIVE";

/// A hidden slider in the HUD bound to a ConVar nothing on a client reads. All are SourceTV
/// server settings of the engine in the player's own game process: `release`, not archived
/// (nothing lands in `user_convars_*.vcfg`), not cheat, not dev-only, not replicated, and
/// read only by a SourceTV server a client never runs, so any value, 0 included, is inert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub convar: &'static str,
    /// The game's default, set back at the end of a session.
    pub default: &'static str,
    pub max: u32,
    /// The slider panel's id in the HUD layout.
    pub id: &'static str,
}

/// Sequence (10 bits) and chunk number (10 bits); chunk 0 means "no chunk".
pub const CTL: Slot = Slot {
    convar: "tv_chattimelimit",
    default: "0.2",
    max: (1 << 20) - 1,
    id: "DtLiveCtl",
};
/// One 16-bit word each.
pub const DATA: [Slot; 10] = [
    Slot {
        convar: "tv_broadcast_spew_threshold",
        default: "0.1",
        max: WORD_MAX,
        id: "DtLiveD0",
    },
    Slot {
        convar: "tv_maxclients",
        default: "128",
        max: WORD_MAX,
        id: "DtLiveD1",
    },
    Slot {
        convar: "tv_broadcast_keyframe_interval",
        default: "3",
        max: WORD_MAX,
        id: "DtLiveD2",
    },
    Slot {
        convar: "tv_broadcast_keyframe_interval1",
        default: "3",
        max: WORD_MAX,
        id: "DtLiveD3",
    },
    Slot {
        convar: "tv_broadcast_startup_resend_interval",
        default: "10",
        max: WORD_MAX,
        id: "DtLiveD4",
    },
    Slot {
        convar: "tv_broadcast_max_requests",
        default: "20",
        max: WORD_MAX,
        id: "DtLiveD5",
    },
    Slot {
        convar: "tv_broadcast_max_requests1",
        default: "20",
        max: WORD_MAX,
        id: "DtLiveD6",
    },
    Slot {
        convar: "tv_chatgroupsize",
        default: "0",
        max: WORD_MAX,
        id: "DtLiveD7",
    },
    Slot {
        convar: "tv_maxrate",
        default: "0",
        max: WORD_MAX,
        id: "DtLiveD8",
    },
    Slot {
        convar: "tv_timeout",
        default: "20",
        max: WORD_MAX,
        id: "DtLiveD9",
    },
];
pub const WORD_MAX: u32 = u16::MAX as u32;
/// A second slider on the control ConVar inside a collapsed panel, so one hello line tells
/// whether a collapsed slider still follows its ConVar.
pub const PROBE_ID: &str = "DtLiveProbe";
pub const SLOTS_ID: &str = "DtLive";
/// The panel in the game's `hud.vxml_c` our slots go after.
const SLOTS_ANCHOR: &str = "TopBar";
/// Words of chunk 1 taken by the header: count and kind, byte length, base (two words).
const HEADER_WORDS: usize = 4;
const SEQ_BITS: u32 = 10;
pub const SEQ_MOD: u32 = 1 << SEQ_BITS;
const CHUNK_MASK: u32 = (1 << 10) - 1;
/// Dictionary tokens are bytes from this value up; the payload itself is ASCII below it.
const TOKEN_BASE: usize = 0x80;
const TOKENS: usize = 0x100 - TOKEN_BASE;

/// How long a chunk may wait for the script's answer before the message is sent again.
/// Longer than a pull round trip (the script execs every `PULL_SECS`, DeadTune reads the
/// log four times a second) plus the console log's delay.
pub const TIMEOUT: Duration = Duration::from_secs(3);
/// The script reads the sliders this often.
pub const POLL_SECS: f64 = 0.25;
/// While a message with the pull flag is coming in, the script execs the cfg this often.
pub const PULL_SECS: f64 = 0.3;
/// The script stops pulling this long after the last chunk it received.
pub const PULL_TIMEOUT_SECS: f64 = 3.0;
pub const CFG_NAME: &str = "deadtune_hud.cfg";
pub const OWN_SCRIPT: &str = "panorama/scripts/deadtune/live_hud.vjs_c";
/// The HUD root layout; our script goes into its `<scripts>`, the slots after `#TopBar`.
pub const HUD_LAYOUT: &str = "panorama/layout/hud.vxml_c";
const SCRIPT: &str = include_str!("assets/live_hud.js");

/// Properties the script sets on panels.
pub const LIVE_PROPS: &[&str] = &[
    "transform",
    "transform-origin",
    "ui-scale",
    "pre-transform-scale2d",
    "opacity",
    "visibility",
    "wash-color",
    "background-color",
    "saturation",
    "brightness",
    "contrast",
    "hue-rotation",
    "width",
    "height",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "font-size",
    "color",
    "border-radius",
];

/// The value that undoes a property when a baked rule is dropped. `ui-scale` and
/// `visibility` on an element's selector come from `ELEMENTS` instead (`reset`).
pub const RESETS: &[(&str, &str)] = &[
    ("transform", "none"),
    ("transform-origin", "50% 50%"),
    ("opacity", "1"),
    ("ui-scale", "100%"),
    ("pre-transform-scale2d", "1"),
    ("saturation", "1"),
    ("brightness", "1"),
    ("contrast", "1"),
    ("hue-rotation", "0deg"),
    ("visibility", "visible"),
];

/// Payload fragments every pak's dictionary starts with, most common first.
const FRAGMENTS: &[&str] = &[
    "^transform^translateX(",
    "px) translateY(",
    "px)",
    "^opacity^0.",
    "^opacity^",
    "^ui-scale^",
    "^visibility^collapse",
    "^visibility^visible",
    "^pre-transform-scale2d^",
    "^transform-origin^",
    "^wash-color^#",
    "^background-color^",
    "^saturation^",
    "^brightness^",
    "^contrast^",
    "^hue-rotation^",
    "^width^",
    "^height^",
    "^font-size^",
    "^color^#",
    "^border-radius^",
    "^margin",
    "~#",
    "#hud_minimap ",
    ".map_button",
    "#BackgroundImage",
    ".HealthVisible",
    "rgba(",
    "none",
    "100%",
    "deg",
    "px~",
    "%~",
    "scale(",
];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LiveRule {
    pub selector: String,
    pub prop: String,
    pub value: String,
}

impl LiveRule {
    fn key(&self) -> (String, String) {
        (self.selector.clone(), self.prop.clone())
    }
}

/// A part of the layout the live preview cannot show; it needs Apply and a restart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotLive {
    pub feature: HudFeature,
    pub why: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overrides {
    pub rules: Vec<LiveRule>,
    /// Baked keys the desired layout dropped that have no live reset.
    pub not_resettable: Vec<LiveRule>,
}

/// `Full` replaces the script's whole override set; `Patch` merges into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Full,
    Patch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub seq: u32,
    pub base: String,
    pub kind: Kind,
    /// The script fetches the rest of the message by running `exec deadtune_hud` itself.
    pub pull: bool,
    pub rules: Vec<LiveRule>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("no chunks")]
    Empty,
    #[error("chunk {0} is not {1} words")]
    Width(usize, usize),
    #[error("chunk 1 is missing")]
    NoHeader,
    #[error("chunk {0} of {1} is missing")]
    Missing(usize, usize),
    #[error("bad record: {0}")]
    Record(String),
    #[error("payload is not text")]
    Text,
}

/// What the script echoes into the console log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveLine {
    Hello {
        base: String,
        /// `key=value` probe readings, verbatim, for the Windows checks.
        probes: String,
    },
    Ok {
        seq: u32,
        base: String,
    },
    WrongBase {
        seq: u32,
        base: String,
    },
    /// Part of a long message arrived: `have` of its chunks so far.
    Got {
        seq: u32,
        have: usize,
        base: String,
    },
}

/// Why a declaration is not live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Fail {
    Selector,
    Property,
    NoReset,
}

impl Fail {
    fn text(self) -> &'static str {
        match self {
            Fail::Selector => "selectors the live preview cannot match, such as :hover or >",
            Fail::Property => "properties it cannot set",
            Fail::NoReset => "changes it cannot undo when the game state changes",
        }
    }
}

/// What the script's selector matcher needs to know about a selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    classed: bool,
    /// Ids, classes, tags: the cascade order the script reproduces by applying in order.
    specificity: (u16, u16, u16),
}

fn ident_len(s: &str) -> usize {
    s.bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        .count()
}

/// `None` unless every compound is `Tag`, `#id`, `.class` and `:not(.class)` tokens, the
/// grammar of the script's one tokenising regex.
fn shape(selector: &str) -> Option<Shape> {
    let (mut ids, mut classes, mut tags) = (0, 0, 0);
    let mut compounds = 0;
    for compound in selector.split_whitespace() {
        compounds += 1;
        let mut rest = compound;
        if rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            tags += 1;
            rest = &rest[ident_len(rest)..];
        }
        while !rest.is_empty() {
            let (is_id, closing, body) = if let Some(r) = rest.strip_prefix(":not(.") {
                (false, ")", r)
            } else if let Some(r) = rest.strip_prefix('#') {
                (true, "", r)
            } else {
                (false, "", rest.strip_prefix('.')?)
            };
            let n = ident_len(body);
            if n == 0 {
                return None;
            }
            rest = body[n..].strip_prefix(closing)?;
            if is_id {
                ids += 1;
            } else {
                classes += 1;
            }
        }
    }
    (compounds > 0).then_some(Shape {
        classed: classes > 0,
        specificity: (ids, classes, tags),
    })
}

/// The value that undoes `prop` on `selector`, or `None` when there is none.
pub fn reset(selector: &str, prop: &str) -> Option<String> {
    let element = ELEMENTS
        .iter()
        .find(|e| e.selector.split(',').any(|s| s.trim() == selector));
    match (prop, element) {
        ("ui-scale", Some(e)) => Some(format!("{}%", e.vanilla_ui_scale_pct)),
        ("visibility", Some(e)) if e.vanilla.collapsed => Some("collapse".to_string()),
        _ => RESETS
            .iter()
            .find(|(p, _)| *p == prop)
            .map(|(_, v)| v.to_string()),
    }
}

fn classify(selector: &str, prop: &str) -> Result<Shape, Fail> {
    let shape = shape(selector).ok_or(Fail::Selector)?;
    if !LIVE_PROPS.contains(&prop) {
        return Err(Fail::Property);
    }
    if shape.classed && reset(selector, prop).is_none() {
        return Err(Fail::NoReset);
    }
    Ok(shape)
}

fn specificity(rule: &LiveRule) -> (u16, u16, u16) {
    shape(&rule.selector).map_or((0, 0, 0), |s| s.specificity)
}

/// Live rules of a compiled patch and why each other declaration is not live. A later
/// declaration of the same key replaces an earlier one; the result is ordered by
/// specificity, then source order, so applying it in order follows the cascade.
fn split(patch: &HudPatch) -> Result<(Vec<LiveRule>, Vec<Fail>), LayoutError> {
    let mut live = Vec::new();
    let mut not_live = Vec::new();
    for (path, text) in &patch.styles {
        let parsed = css::parse_rules(text).map_err(|e| LayoutError::ExtraCss(path.clone(), e))?;
        for css_rule in parsed {
            for selector in css_rule.selectors.split(',') {
                let selector = selector.split_whitespace().collect::<Vec<_>>().join(" ");
                for (prop, value) in &css_rule.decls {
                    let rule = LiveRule {
                        selector: selector.clone(),
                        prop: prop.clone(),
                        value: value.clone(),
                    };
                    match classify(&selector, prop) {
                        Ok(_) => live.push(rule),
                        Err(fail) => not_live.push(fail),
                    }
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    live.reverse();
    live.retain(|r| seen.insert(r.key()));
    live.reverse();
    live.sort_by_key(specificity);
    Ok((live, not_live))
}

/// The layout's live rules: everything Apply would bake that the script can set.
pub fn rules(layout: &HudLayout) -> Result<Vec<LiveRule>, LayoutError> {
    Ok(split(&layout::compile(layout)?)?.0)
}

/// The layout with only `feature` kept, so not-live rules can be put down to a feature.
fn only(layout: &HudLayout, feature: HudFeature) -> HudLayout {
    let mut one = HudLayout::default();
    match feature {
        HudFeature::Layout => one.elements = layout.elements.clone(),
        HudFeature::MinimapColors => one.minimap_colors = layout.minimap_colors.clone(),
        HudFeature::MinimapStyle => one.minimap = layout.minimap.clone(),
        HudFeature::TopBar => one.top_bar = layout.top_bar.clone(),
        HudFeature::HealthBar => one.health = layout.health.clone(),
        HudFeature::PlayerStats => one.player_stats = layout.player_stats.clone(),
        HudFeature::ApplesTunnels => one.apples_tunnels = layout.apples_tunnels,
        HudFeature::IngameSettings => one.ingame = layout.ingame.clone(),
        HudFeature::CustomCss => one.extra_css = layout.extra_css.clone(),
        HudFeature::Images => one.icons = layout.icons.clone(),
        HudFeature::LivePreview => {}
    }
    one
}

/// What a feature adds that the game only builds at start.
fn restart_only(layout: &HudLayout, feature: HudFeature) -> Option<&'static str> {
    match feature {
        HudFeature::TopBar if layout.top_bar.has_extras() => {
            Some("spawn timers, urn soul lead and purchase popups load when the game starts")
        }
        HudFeature::ApplesTunnels => Some("its minimap markers load when the game starts"),
        HudFeature::IngameSettings => Some("its rows in the game's menu load when the game starts"),
        HudFeature::Images => Some("replaced images load when the game starts"),
        _ => None,
    }
}

fn fail_text(fails: &[Fail]) -> String {
    let n = fails.len();
    let causes: BTreeSet<Fail> = fails.iter().copied().collect();
    let causes: Vec<&str> = causes.into_iter().map(Fail::text).collect();
    let (s, verb) = if n == 1 { ("", "needs") } else { ("s", "need") };
    format!(
        "{n} style change{s} {verb} a restart: {}",
        causes.join(", ")
    )
}

/// The features of `layout` that need Apply and a restart, each with a short reason.
pub fn coverage(layout: &HudLayout) -> Vec<NotLive> {
    layout
        .features()
        .into_iter()
        .filter_map(|feature| {
            let mut why: Vec<String> = restart_only(layout, feature)
                .map(str::to_string)
                .into_iter()
                .collect();
            if let Ok((_, fails)) = layout::compile(&only(layout, feature)).and_then(|p| split(&p))
                && !fails.is_empty()
            {
                why.push(fail_text(&fails));
            }
            (!why.is_empty()).then(|| NotLive {
                feature,
                why: why.join("; "),
            })
        })
        .collect()
}

/// What to send so the game shows `desired` on top of a pak that baked `baked`: every live
/// rule of `desired`, and a reset for each baked key `desired` dropped.
pub fn overrides(desired: &HudLayout, baked: &HudLayout) -> Result<Overrides, LayoutError> {
    let mut out = Overrides {
        rules: rules(desired)?,
        not_resettable: Vec::new(),
    };
    let wanted: BTreeSet<(String, String)> = out.rules.iter().map(LiveRule::key).collect();
    for had in rules(baked)? {
        if wanted.contains(&had.key()) {
            continue;
        }
        match reset(&had.selector, &had.prop) {
            Some(value) => out.rules.push(LiveRule { value, ..had }),
            None => out.not_resettable.push(had),
        }
    }
    out.rules.sort_by_key(specificity);
    Ok(out)
}

/// Eight hex digits naming the rules a pak bakes and the dictionary its script carries,
/// the same with the live switch on or off.
pub fn base_id(layout: &HudLayout) -> Result<String, LayoutError> {
    let without = HudLayout {
        live: false,
        ..layout.clone()
    };
    Ok(patch_base(&layout::compile(&without)?))
}

/// `base_id` of a patch compiled without the live switch.
fn patch_base(patch: &HudPatch) -> String {
    let mut hash = Sha256::new();
    for (path, css) in &patch.styles {
        hash.update(format!("{path}\0{css}\0"));
    }
    for (path, edit) in &patch.layouts {
        hash.update(format!("{path}\0{edit:?}\0"));
    }
    for token in patch_dictionary(patch) {
        hash.update(format!("{token}\0"));
    }
    hash.finalize()[..4]
        .iter()
        .fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// Kept as is in a field; `%`, the record separators `~` and `^`, control characters and
/// anything outside ASCII are `%XX` per UTF-8 byte, so the payload is ASCII below the
/// dictionary's token range.
fn plain(c: char) -> bool {
    c.is_ascii_graphic() && !"%~^".contains(c) || c == ' '
}

fn escape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    for c in field.chars() {
        if plain(c) {
            out.push(c);
        } else {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn unescape(field: &str) -> Option<String> {
    let b = field.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn payload(rules: &[LiveRule]) -> String {
    rules
        .iter()
        .map(|r| {
            [&r.selector, &r.prop, &r.value]
                .map(|f| escape(f))
                .join("^")
        })
        .collect::<Vec<_>>()
        .join("~")
}

fn parse_payload(text: &str) -> Result<Vec<LiveRule>, CodecError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.split('~')
        .map(|record| {
            let bad = || CodecError::Record(record.to_string());
            let fields: Vec<String> = record
                .split('^')
                .map(|f| unescape(f).ok_or_else(bad))
                .collect::<Result<_, _>>()?;
            let [selector, prop, value] = <[String; 3]>::try_from(fields).map_err(|_| bad())?;
            Ok(LiveRule {
                selector,
                prop,
                value,
            })
        })
        .collect()
}

/// The payload fragments a pak's script expands from one byte each: `FRAGMENTS`, every
/// element selector, every live property, then what the pak's own rules use. Deterministic
/// from the compiled patch, so DeadTune and the script agree on it through `base_id`.
fn patch_dictionary(patch: &HudPatch) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |token: &str| {
        if token.len() >= 2 && token.is_ascii() && !out.iter().any(|t| t == token) {
            out.push(token.to_string());
        }
    };
    for f in FRAGMENTS {
        add(f);
    }
    for e in ELEMENTS {
        for part in e.selector.split(',') {
            add(part.trim());
        }
    }
    for p in LIVE_PROPS {
        add(&format!("^{p}^"));
    }
    if let Ok((baked, _)) = split(patch) {
        for r in &baked {
            add(&escape(&r.selector));
        }
        for r in &baked {
            add(&escape(&r.value));
        }
    }
    out.truncate(TOKENS);
    out
}

/// The dictionary the script of a pak that baked `layout` carries.
pub fn dictionary(layout: &HudLayout) -> Result<Vec<String>, LayoutError> {
    let without = HudLayout {
        live: false,
        ..layout.clone()
    };
    Ok(patch_dictionary(&layout::compile(&without)?))
}

/// The payload as bytes: ASCII as is, dictionary tokens as one byte each (longest match).
fn compress(text: &str, dict: &[String]) -> Vec<u8> {
    let mut by_len: Vec<(usize, &str)> = dict.iter().map(String::as_str).enumerate().collect();
    by_len.sort_by_key(|(i, t)| (std::cmp::Reverse(t.len()), *i));
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match by_len
            .iter()
            .find(|(_, t)| bytes[i..].starts_with(t.as_bytes()))
        {
            Some((id, t)) => {
                out.push((TOKEN_BASE + id) as u8);
                i += t.len();
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    out
}

fn expand(bytes: &[u8], dict: &[String]) -> Result<String, CodecError> {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        if (b as usize) < TOKEN_BASE {
            out.push(b as char);
        } else {
            out.push_str(dict.get(b as usize - TOKEN_BASE).ok_or(CodecError::Text)?);
        }
    }
    Ok(out)
}

fn base_words(base: &str) -> (u32, u32) {
    let n = u32::from_str_radix(base, 16).unwrap_or(0);
    (n >> 16, n & WORD_MAX)
}

/// The message as chunks of `DATA.len()` words. Chunk 1 starts with the header: chunk
/// count, pull flag and kind in one word, the payload's byte length, the base in two.
pub fn encode(msg: &Message, dict: &[String]) -> Vec<Vec<u32>> {
    let bytes = compress(&payload(&msg.rules), dict);
    let (hi, lo) = base_words(&msg.base);
    let mut words = vec![0, bytes.len() as u32, hi, lo];
    for pair in bytes.chunks(2) {
        words.push(u32::from(pair[0]) << 8 | u32::from(*pair.get(1).unwrap_or(&0)));
    }
    let mut chunks: Vec<Vec<u32>> = words.chunks(DATA.len()).map(<[u32]>::to_vec).collect();
    for chunk in &mut chunks {
        chunk.resize(DATA.len(), 0);
    }
    let kind = u32::from(msg.kind == Kind::Full);
    chunks[0][0] = (chunks.len() as u32) << 2 | u32::from(msg.pull) << 1 | kind;
    chunks
}

/// The message the chunks carry, in order; what the script does in game.
pub fn decode(chunks: &[Vec<u32>], dict: &[String]) -> Result<Message, CodecError> {
    let first = chunks.first().ok_or(CodecError::Empty)?;
    if let Some(bad) = chunks.iter().position(|c| c.len() != DATA.len()) {
        return Err(CodecError::Width(bad + 1, DATA.len()));
    }
    if first.len() < HEADER_WORDS {
        return Err(CodecError::NoHeader);
    }
    let n = (first[0] >> 2) as usize;
    if n == 0 {
        return Err(CodecError::NoHeader);
    }
    if chunks.len() < n {
        return Err(CodecError::Missing(chunks.len() + 1, n));
    }
    let len = first[1] as usize;
    let base = format!("{:04x}{:04x}", first[2], first[3]);
    let mut bytes = Vec::with_capacity(len);
    for word in first[HEADER_WORDS..]
        .iter()
        .chain(chunks[1..n].iter().flatten())
    {
        bytes.push((word >> 8) as u8);
        bytes.push((word & 0xff) as u8);
    }
    bytes.truncate(len);
    let text = expand(&bytes, dict)?;
    // The sequence travels in the control slot, not in the words.
    Ok(Message {
        seq: 0,
        base,
        kind: if first[0] & 1 == 1 {
            Kind::Full
        } else {
            Kind::Patch
        },
        pull: first[0] & 2 == 2,
        rules: parse_payload(&text)?,
    })
}

/// The slot settings that put chunk `i` (1-based) of message `seq` in front of the script.
pub fn chunk_cmds(seq: u32, i: usize, words: &[u32]) -> Vec<ConsoleCmd> {
    let mut out = vec![ConsoleCmd {
        name: CTL.convar.to_string(),
        value: ((seq % SEQ_MOD) << 10 | i as u32 & CHUNK_MASK).to_string(),
    }];
    out.extend(DATA.iter().enumerate().map(|(k, slot)| ConsoleCmd {
        name: slot.convar.to_string(),
        value: words.get(k).copied().unwrap_or(0).to_string(),
    }));
    out
}

/// Every slot back to the game's default, for the end of a session.
pub fn reset_cmds() -> Vec<ConsoleCmd> {
    std::iter::once(&CTL)
        .chain(DATA.iter())
        .map(|slot| ConsoleCmd {
            name: slot.convar.to_string(),
            value: slot.default.to_string(),
        })
        .collect()
}

/// Distinct values for every slot, set at game start so the script's hello line shows
/// which sliders read their ConVar: the control slot as sequence 1, chunk 0 (no chunk),
/// data slot `k` as `1000 + k`.
pub fn probe_cmds() -> Vec<ConsoleCmd> {
    let mut out = vec![ConsoleCmd {
        name: CTL.convar.to_string(),
        value: (1 << 10).to_string(),
    }];
    out.extend(DATA.iter().enumerate().map(|(k, slot)| ConsoleCmd {
        name: slot.convar.to_string(),
        value: (1000 + k).to_string(),
    }));
    out
}

/// The console line that makes the game read `CFG_NAME`, by bare name like the script's.
pub fn exec_line() -> String {
    format!("exec {}", CFG_NAME.trim_end_matches(".cfg"))
}

/// A line the script echoed, tolerant of whatever the log puts before the marker.
pub fn parse_line(line: &str) -> Option<LiveLine> {
    let at = line.find(LIVE)?;
    let rest = line[at + LIVE.len()..].trim();
    let mut words = rest.split_whitespace();
    let first = words.next()?;
    if first == "hello" {
        let base = words.next()?.to_string();
        let probes = rest[rest.find(&base)? + base.len()..].trim().to_string();
        return Some(LiveLine::Hello { base, probes });
    }
    let seq = first.parse().ok()?;
    match words.next()? {
        "ok" => Some(LiveLine::Ok {
            seq,
            base: words.next()?.to_string(),
        }),
        "wrongbase" => Some(LiveLine::WrongBase {
            seq,
            base: words.next()?.to_string(),
        }),
        "got" => Some(LiveLine::Got {
            seq,
            have: words.next()?.parse().ok()?,
            base: words.next()?.to_string(),
        }),
        _ => None,
    }
}

/// Writes `cmds` as the file `exec deadtune_hud` reads; no commands empties it, which is a
/// harmless exec.
pub fn write_cfg(cfg_dir: &Path, cmds: &[ConsoleCmd]) -> Result<(), BridgeError> {
    let lines: Vec<String> = cmds
        .iter()
        .map(ConsoleCmd::to_line)
        .collect::<Result<_, _>>()?;
    Ok(execfile::write_cfg(cfg_dir, CFG_NAME, &lines)?)
}

fn js_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c.is_ascii_graphic() || c == ' ' => out.push(c),
            c => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
        }
    }
    out.push('"');
    out
}

/// Our script for a pak whose rules hash to `base` and whose dictionary is `dict`.
pub fn script(base: &str, dict: &[String]) -> String {
    let (hi, lo) = base_words(base);
    let data = DATA.map(|s| js_string(s.id)).join(", ");
    let dict = dict
        .iter()
        .map(|t| js_string(t))
        .collect::<Vec<_>>()
        .join(", ");
    let cfg = CFG_NAME.trim_end_matches(".cfg");
    format!(
        "var DT_LIVE = {{ base: \"{base}\", baseWords: [{hi}, {lo}], ctl: \"{}\", ctlMax: {}, probe: \"{PROBE_ID}\", data: [{data}], max: {WORD_MAX}, dict: [{dict}], cfg: \"{cfg}\", poll: {POLL_SECS}, pull: {PULL_SECS}, pullTimeout: {PULL_TIMEOUT_SECS}, webPort: {} }};\n{SCRIPT}",
        CTL.id,
        CTL.max,
        super::web_probe::PORT
    )
}

fn slider(slot: &Slot) -> Element {
    Element::new("CitadelSettingsSlider")
        .attr("id", slot.id)
        .attr("class", "VideoPreview")
        .attr("convar", slot.convar)
        .attr("min", "0")
        .attr("max", &slot.max.to_string())
        .attr("snap", "1")
        .attr("percentage", "false")
        .attr("displayprecision", "0")
        .attr("text", slot.id)
        .attr("textentry", "true")
}

/// The hidden sliders: a clipped, transparent panel that takes no room, plus one slider
/// on the control ConVar inside a collapsed panel for the hello line's probe.
pub fn slots_panel() -> Element {
    let mut panel = Element::new("Panel")
        .attr("id", SLOTS_ID)
        .attr("hittest", "false")
        .attr("hittestchildren", "false")
        .attr(
            "style",
            "width: 0px; height: 0px; overflow: clip; opacity: 0;",
        )
        .child(slider(&CTL));
    for slot in &DATA {
        panel = panel.child(slider(slot));
    }
    let probe = Slot {
        id: PROBE_ID,
        ..CTL
    };
    panel.child(
        Element::new("Panel")
            .attr("id", "DtLiveCollapsed")
            .attr("style", "visibility: collapse;")
            .child(slider(&probe)),
    )
}

/// Adds the script and the slots to a patch compiled without them.
pub(crate) fn add_to(patch: &mut HudPatch) {
    let base = patch_base(patch);
    let dict = patch_dictionary(patch);
    let edit: &mut LayoutEdit = patch.layouts.entry(HUD_LAYOUT.to_string()).or_default();
    edit.script_includes.push(format!("s2r://{OWN_SCRIPT}"));
    edit.panels
        .push((Anchor::After(SLOTS_ANCHOR.into()), slots_panel()));
    patch
        .own_files
        .insert(OWN_SCRIPT.to_string(), script(&base, &dict));
}

/// A stand-in for the game's HUD layout, for tests and the fake install.
pub fn stand_in_layout() -> Element {
    Element::new("root")
        .child(Element::new("styles").child(Element::include("s2r://panorama/styles/hud.vcss")))
        .child(
            Element::new("CitadelHud")
                .attr("id", "Hud")
                .child(Element::new("Panel").attr("id", "TopBar"))
                .child(Element::new("Panel").attr("id", "minimap_persp"))
                .child(Element::new("Panel").attr("id", "Chat")),
        )
}

/// One chunk to deliver: its slot settings, and whether it opens a message the bound key
/// has to carry (`exec deadtune_hud` goes through the bridge once per key press).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub seq: u32,
    pub i: usize,
    pub n: usize,
    pub cmds: Vec<ConsoleCmd>,
    pub needs_key: bool,
}

#[derive(Clone, Debug)]
struct Flight {
    seq: u32,
    /// The override set the script shows once it applies this message.
    target: Vec<LiveRule>,
    chunks: Vec<Vec<u32>>,
    /// Chunks handed out so far.
    sent: usize,
    /// Chunks the script reported holding.
    received: usize,
    at: Instant,
}

/// One message in flight at a time, one chunk per batch. It keeps the override set the game
/// should show (`desired`) and the set the script last acknowledged (`applied`), so it
/// decides itself between a patch and a full message, and a lost message is made good by
/// sending everything again. Pure: the caller delivers the batches and feeds back the
/// script's lines.
#[derive(Clone, Debug)]
pub struct Mailbox {
    base: String,
    dict: Vec<String>,
    next_seq: u32,
    desired: Vec<LiveRule>,
    applied: Option<Vec<LiveRule>>,
    flight: Option<Flight>,
    /// A chunk 1 went out through the bound key and the game has not run it yet.
    key_pending: bool,
}

impl Mailbox {
    /// `first_seq` should differ between DeadTune runs (seed it from the clock): the script
    /// ignores a message whose seq it applied last.
    pub fn new(base: &str, dict: Vec<String>, first_seq: u32) -> Mailbox {
        Mailbox {
            base: base.to_string(),
            dict,
            next_seq: first_seq % SEQ_MOD,
            desired: Vec::new(),
            // The script may still hold an earlier DeadTune run's overrides, so the first
            // message replaces whatever it has.
            applied: None,
            flight: None,
            key_pending: false,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// The override set the game should show from now on, and the first batch when one
    /// can go. A message the script has not started reading is replaced outright.
    pub fn send(&mut self, desired: Vec<LiveRule>, now: Instant, pull: bool) -> Option<Batch> {
        self.desired = desired;
        if self.flight.as_ref().is_some_and(|f| f.received == 0) {
            self.flight = None;
        }
        self.poll(now, pull)
    }

    /// The script loaded again (a hello while live): it shows the baked HUD now.
    pub fn reload(&mut self) {
        self.applied = None;
        self.flight = None;
    }

    /// The next batch to deliver, if any: the next chunk once the script holds the last
    /// one, or the next message once the last is acked. With `pull` the game reads chunk 1
    /// only when the player presses the bound key, so it waits as long as it takes; a chunk
    /// left unanswered for `TIMEOUT` after the script started reading makes the whole
    /// message go again.
    pub fn poll(&mut self, now: Instant, pull: bool) -> Option<Batch> {
        if let Some(flight) = &mut self.flight {
            if flight.received == flight.sent && flight.sent < flight.chunks.len() {
                let i = flight.sent + 1;
                flight.sent = i;
                flight.at = now;
                return Some(Batch {
                    seq: flight.seq,
                    i,
                    n: flight.chunks.len(),
                    cmds: chunk_cmds(flight.seq, i, &flight.chunks[i - 1]),
                    needs_key: false,
                });
            }
            let stalled = now.duration_since(flight.at) >= TIMEOUT;
            if !stalled || (pull && flight.received == 0) {
                return None;
            }
            self.flight = None;
            self.applied = None;
        }
        let (kind, rules) = match &self.applied {
            Some(applied)
                if applied
                    .iter()
                    .all(|a| self.desired.iter().any(|d| d.key() == a.key())) =>
            {
                let changed: Vec<LiveRule> = self
                    .desired
                    .iter()
                    .filter(|d| !applied.contains(d))
                    .cloned()
                    .collect();
                if changed.is_empty() {
                    return None;
                }
                (Kind::Patch, changed)
            }
            _ => (Kind::Full, self.desired.clone()),
        };
        let seq = self.next_seq;
        self.next_seq = (seq + 1) % SEQ_MOD;
        let chunks = encode(
            &Message {
                seq,
                base: self.base.clone(),
                kind,
                pull,
                rules,
            },
            &self.dict,
        );
        let batch = Batch {
            seq,
            i: 1,
            n: chunks.len(),
            cmds: chunk_cmds(seq, 1, &chunks[0]),
            needs_key: pull && !self.key_pending,
        };
        self.key_pending = pull;
        self.flight = Some(Flight {
            seq,
            target: self.desired.clone(),
            chunks,
            sent: 1,
            received: 0,
            at: now,
        });
        Some(batch)
    }

    /// The script applied message `seq`.
    pub fn ack(&mut self, seq: u32) {
        self.key_pending = false;
        if let Some(flight) = self.flight.take_if(|f| f.seq == seq) {
            self.applied = Some(flight.target);
        }
    }

    /// The script holds `have` chunks of message `seq`.
    pub fn got(&mut self, seq: u32, have: usize) {
        self.key_pending = false;
        if let Some(flight) = self.flight.as_mut().filter(|f| f.seq == seq) {
            flight.received = flight.received.max(have);
        }
    }

    /// The script saw a message of ours; whatever it said, the key was pressed.
    pub fn heard(&mut self) {
        self.key_pending = false;
    }

    /// A message is in flight or waiting to go.
    pub fn pending(&self) -> bool {
        self.flight.is_some() || self.applied.as_ref() != Some(&self.desired)
    }

    pub fn key_pending(&self) -> bool {
        self.key_pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::elements::{ELEMENTS, HUD_STYLE};
    use crate::hud::health_style::HealthPreset;
    use crate::hud::ingame::IngameSettings;
    use crate::hud::inject;
    use crate::hud::layout::{self, ElementEdit, Visibility};
    use crate::hud::minimap_colors::{Color, IconId};
    use crate::hud::minimap_style::MarkerGroup;
    use crate::hud::player_stats::StatsPreset;
    use crate::hud::topbar::{DeadLook, TopBarPreset, TopBarStyle, Treatment};

    fn rule(selector: &str, prop: &str, value: &str) -> LiveRule {
        LiveRule {
            selector: selector.into(),
            prop: prop.into(),
            value: value.into(),
        }
    }

    fn msg(seq: u32, kind: Kind, rules: Vec<LiveRule>) -> Message {
        Message {
            seq,
            base: "0badf00d".into(),
            kind,
            pull: false,
            rules,
        }
    }

    fn many(n: usize) -> Vec<LiveRule> {
        (0..n)
            .map(|i| {
                rule(
                    &format!("#hud_minimap .map_button.player #Icon{i}"),
                    "transform",
                    &format!("translateX({i}px) translateY(-{i}px)"),
                )
            })
            .collect()
    }

    fn dict() -> Vec<String> {
        dictionary(&HudLayout::default()).unwrap()
    }

    fn element_layout(edits: &[(crate::hud::ElementId, ElementEdit)]) -> HudLayout {
        HudLayout {
            elements: edits.iter().cloned().collect(),
            ..HudLayout::default()
        }
    }

    fn moved(x: i32) -> HudLayout {
        element_layout(&[(
            crate::hud::ElementId::Minimap,
            ElementEdit {
                offset_x: x,
                ..ElementEdit::default()
            },
        )])
    }

    #[test]
    fn codec_round_trips_every_escaped_character() {
        let nasty = rule(
            "#a .b:not(.c)",
            "font-family",
            "100% ~ ^ \" ; \r\n %25 ü {x} // end ",
        );
        let m = msg(7, Kind::Patch, vec![nasty, rule("#X", "opacity", "")]);
        let text = payload(&m.rules);
        assert_eq!(text.matches('~').count(), 1, "{text}");
        assert_eq!(text.matches('^').count(), 4, "{text}");
        assert!(text.is_ascii());
        for c in ['\r', '\n', 'ü', '\t'] {
            assert!(!text.contains(c), "{c:?} in {text}");
        }
        let chunks = encode(&m, &dict());
        let got = decode(&chunks, &dict()).unwrap();
        assert_eq!(
            (got.kind, got.base.as_str(), got.rules),
            (m.kind, "0badf00d", m.rules)
        );
        let empty = Message {
            pull: true,
            ..msg(8, Kind::Full, Vec::new())
        };
        let chunks = encode(&empty, &dict());
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0][..HEADER_WORDS],
            [1 << 2 | 2 | 1, 0, 0x0bad, 0xf00d]
        );
        assert!(chunks[0][HEADER_WORDS..].iter().all(|w| *w == 0));
        let got = decode(&chunks, &dict()).unwrap();
        assert_eq!((got.kind, got.pull, got.rules.len()), (Kind::Full, true, 0));
    }

    #[test]
    fn the_dictionary_makes_one_element_edit_one_chunk() {
        let d = dict();
        assert!(d.len() <= TOKENS, "{}", d.len());
        assert!(d.iter().all(|t| t.len() >= 2 && t.is_ascii()));
        assert_eq!(d[0], FRAGMENTS[0]);
        assert!(d.contains(&"#minimap_persp".to_string()));
        assert!(d.contains(&"^opacity^".to_string()));
        let drag = rules(&moved(-560)).unwrap();
        assert_eq!(drag.len(), 1, "{drag:?}");
        let chunks = encode(&msg(1, Kind::Patch, drag.clone()), &d);
        assert_eq!(chunks.len(), 1, "{:?}", payload(&drag));
        let bytes = compress(&payload(&drag), &d);
        assert!(bytes.len() <= 12, "{bytes:?}");
        assert_eq!(expand(&bytes, &d).unwrap(), payload(&drag));
        let plain = compress("zq", &d);
        assert_eq!(plain, b"zq");
    }

    #[test]
    fn a_paks_own_rules_join_its_dictionary_and_change_the_base() {
        let mut l = HudLayout::default();
        l.extra_css
            .insert(HUD_STYLE.into(), "#SomethingNew{opacity:0.123;}".into());
        let d = dictionary(&l).unwrap();
        assert!(d.contains(&"#SomethingNew".to_string()), "{d:?}");
        assert!(d.contains(&"0.123".to_string()), "{d:?}");
        assert_ne!(
            base_id(&l).unwrap(),
            base_id(&HudLayout::default()).unwrap()
        );
    }

    #[test]
    fn long_messages_split_into_chunks_that_reassemble_in_order() {
        let m = msg(41, Kind::Full, many(30));
        let chunks = encode(&m, &dict());
        assert!(chunks.len() > 3, "{}", chunks.len());
        assert_eq!(chunks[0][0] >> 2, chunks.len() as u32);
        for c in &chunks {
            assert_eq!(c.len(), DATA.len());
            assert!(c.iter().all(|w| *w <= WORD_MAX));
        }
        assert_eq!(decode(&chunks, &dict()).unwrap().rules, m.rules);
        assert_eq!(
            decode(&chunks[..2], &dict()),
            Err(CodecError::Missing(3, chunks.len()))
        );
        assert_eq!(decode(&[], &dict()), Err(CodecError::Empty));
        assert_eq!(
            decode(&[vec![0; 3]], &dict()),
            Err(CodecError::Width(1, DATA.len()))
        );
    }

    #[test]
    fn chunk_cmds_set_the_control_then_every_data_slot() {
        let got = chunk_cmds(1023, 2, &[7, 65535]);
        assert_eq!(got.len(), 1 + DATA.len());
        assert_eq!(got[0].name, CTL.convar);
        assert_eq!(got[0].value, (1023 << 10 | 2).to_string());
        assert_eq!(got[1].value, "7");
        assert_eq!(got[2].value, "65535");
        assert!(got[3..].iter().all(|c| c.value == "0"));
        assert_eq!(chunk_cmds(1024, 1, &[])[0].value, "1", "seq wraps to 0");
        for c in &got {
            c.to_line().expect("the console takes every slot");
        }
        let resets = reset_cmds();
        assert_eq!(resets[0].to_line().unwrap(), "tv_chattimelimit \"0.2\"");
        assert_eq!(
            resets[1].to_line().unwrap(),
            "tv_broadcast_spew_threshold \"0.1\""
        );
        assert_eq!(resets.len(), 1 + DATA.len());
        let probes = probe_cmds();
        assert_eq!(probes[0].value, "1024", "sequence 1, no chunk");
        assert_eq!(probes[1].value, "1000");
        assert_eq!(probes[10].value, "1009");
        assert_eq!(exec_line(), "exec deadtune_hud");
    }

    #[test]
    fn slots_use_distinct_safe_convars() {
        let names: BTreeSet<&str> = std::iter::once(CTL.convar)
            .chain(DATA.iter().map(|s| s.convar))
            .collect();
        assert_eq!(names.len(), 1 + DATA.len());
        let list = include_str!("../../../../research/configs/OptimizationLock/cvarlist.txt");
        for name in names {
            let row = list
                .lines()
                .find(|l| l.starts_with(&format!("{name} | ")))
                .unwrap_or_else(|| panic!("{name} is not in the cvarlist"));
            let flags: Vec<&str> = row.split(" | ").nth(1).unwrap().split(", ").collect();
            for bad in ["devonly", "cheat", "hidden", "a", "sv", "rep", "per_user"] {
                assert!(!flags.contains(&bad), "{name} is {flags:?}");
            }
            assert!(flags.contains(&"release"), "{name} is {flags:?}");
            assert!(
                name.starts_with("tv_"),
                "{name}: only SourceTV server settings are sure to do nothing on a client, \
                 at any value including 0"
            );
        }
    }

    #[test]
    fn mailbox_keeps_one_message_in_flight_until_the_ack() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut mb = Mailbox::new("0badf00d", dict(), 5);
        assert!(mb.pending(), "a new session owes the script a full message");
        let first = mb
            .send(vec![rule("#TopBar", "opacity", "0.5")], t0, false)
            .unwrap();
        assert_eq!(
            (first.seq, first.i, first.n, first.needs_key),
            (5, 1, 1, false)
        );
        assert_eq!(decode(&[words(&first)], &dict()).unwrap().kind, Kind::Full);
        assert!(mb.pending());
        assert_eq!(
            mb.poll(ms(500), false),
            None,
            "nothing more until the script answers"
        );
        mb.ack(4);
        assert_eq!(
            mb.poll(ms(600), false),
            None,
            "an old seq does not free the slot"
        );
        mb.ack(5);
        assert_eq!(
            mb.poll(ms(700), false),
            None,
            "desired is what the script shows"
        );
        assert!(!mb.pending());
        let second = mb
            .send(
                vec![
                    rule("#TopBar", "opacity", "0.5"),
                    rule("#Chat", "opacity", "0"),
                ],
                ms(800),
                false,
            )
            .unwrap();
        assert_eq!(second.seq, 6);
        let m = decode(&[words(&second)], &dict()).unwrap();
        assert_eq!(m.kind, Kind::Patch);
        assert_eq!(m.rules, [rule("#Chat", "opacity", "0")], "only the change");
        mb.ack(6);
        let third = mb
            .send(vec![rule("#Chat", "opacity", "0")], ms(900), false)
            .unwrap();
        let m = decode(&[words(&third)], &dict()).unwrap();
        assert_eq!(m.kind, Kind::Full, "a dropped key needs a full message");
        assert_eq!(m.rules, [rule("#Chat", "opacity", "0")]);
    }

    fn words(batch: &Batch) -> Vec<u32> {
        batch.cmds[1..]
            .iter()
            .map(|c| c.value.parse().unwrap())
            .collect()
    }

    #[test]
    fn mailbox_replaces_an_unread_message_and_queues_behind_a_read_one() {
        let t0 = Instant::now();
        let mut mb = Mailbox::new("0badf00d", dict(), 1);
        mb.send(vec![rule("#A", "opacity", "1")], t0, false)
            .unwrap();
        let replaced = mb
            .send(vec![rule("#B", "opacity", "0.1")], t0, false)
            .expect("unread chunk 1 is replaced at once");
        assert_eq!(replaced.seq, 2);
        mb.got(2, 1);
        assert_eq!(
            mb.send(vec![rule("#C", "opacity", "0.3")], t0, false),
            None,
            "the script is reading, so the next waits"
        );
        mb.ack(2);
        let next = mb.poll(t0, false).unwrap();
        let m = decode(&[words(&next)], &dict()).unwrap();
        assert_eq!(m.kind, Kind::Full);
        assert_eq!(m.rules, [rule("#C", "opacity", "0.3")]);
    }

    #[test]
    fn mailbox_sends_long_messages_chunk_by_chunk() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut mb = Mailbox::new("0badf00d", dict(), 9);
        let first = mb.send(many(30), t0, false).unwrap();
        let n = first.n;
        assert!(n > 3, "{n}");
        assert_eq!(mb.poll(ms(100), false), None);
        mb.got(9, 1);
        let second = mb.poll(ms(200), false).unwrap();
        assert_eq!((second.seq, second.i), (9, 2));
        assert_eq!(mb.poll(ms(300), false), None, "chunk 2 is not in yet");
        mb.got(9, 2);
        assert_eq!(mb.poll(ms(400), false).unwrap().i, 3);
        assert_eq!(
            mb.poll(ms(400) + TIMEOUT - Duration::from_millis(1), false),
            None,
            "the timeout counts from the latest chunk"
        );
        let again = mb.poll(ms(400) + TIMEOUT, false).unwrap();
        assert_eq!((again.seq, again.i), (10, 1), "stalled: everything again");
    }

    #[test]
    fn mailbox_timeout_sends_everything_again_as_full() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut mb = Mailbox::new("0badf00d", dict(), 1);
        mb.send(vec![rule("#A", "opacity", "1")], t0, false);
        mb.ack(1);
        mb.send(
            vec![rule("#A", "opacity", "1"), rule("#B", "opacity", "0")],
            ms(10),
            false,
        );
        assert_eq!(
            mb.poll(ms(10) + TIMEOUT - Duration::from_millis(1), false),
            None
        );
        let again = mb.poll(ms(10) + TIMEOUT, false).unwrap();
        let m = decode(&[words(&again)], &dict()).unwrap();
        assert_eq!(again.seq, 3);
        assert_eq!(m.kind, Kind::Full);
        assert_eq!(
            m.rules,
            [rule("#A", "opacity", "1"), rule("#B", "opacity", "0")]
        );
        mb.ack(2);
        assert!(
            mb.pending(),
            "a late ack for the lost message changes nothing"
        );
        mb.ack(3);
        assert!(!mb.pending());
    }

    #[test]
    fn mailbox_in_pull_mode_waits_for_the_key_and_asks_for_it_once() {
        let t0 = Instant::now();
        let mut mb = Mailbox::new("0badf00d", dict(), 1);
        let first = mb.send(vec![rule("#A", "opacity", "1")], t0, true).unwrap();
        assert!(first.needs_key);
        assert!(mb.key_pending());
        assert_eq!(
            mb.poll(t0 + TIMEOUT * 10, true),
            None,
            "chunk 1 waits for the player"
        );
        let replaced = mb
            .send(vec![rule("#A", "opacity", "0.5")], t0, true)
            .unwrap();
        assert_eq!(replaced.seq, 2);
        assert!(!replaced.needs_key, "the key is still owed from before");
        mb.heard();
        assert!(!mb.key_pending());
        mb.ack(2);
        let m = decode(&[words(&replaced)], &dict()).unwrap();
        assert!(m.pull);
        let big = mb.send(many(30), t0, true).unwrap();
        assert!(big.needs_key);
        mb.got(3, 1);
        assert!(!mb.key_pending(), "the game ran the file");
        assert_eq!(mb.poll(t0, true).unwrap().i, 2);
        assert_eq!(
            mb.poll(t0 + TIMEOUT, true).unwrap().i,
            1,
            "a stalled pull starts the message again"
        );
    }

    #[test]
    fn mailbox_reload_starts_from_a_full_message() {
        let t0 = Instant::now();
        let mut mb = Mailbox::new("0badf00d", dict(), 1);
        mb.send(vec![rule("#A", "opacity", "1")], t0, false);
        mb.ack(1);
        assert_eq!(mb.poll(t0, false), None);
        mb.reload();
        assert!(mb.pending());
        let again = mb.poll(t0, false).unwrap();
        assert_eq!(decode(&[words(&again)], &dict()).unwrap().kind, Kind::Full);
    }

    #[test]
    fn parse_line_reads_every_live_line() {
        let cases = [
            (
                "DEADTUNE_LIVE hello 0badf00d",
                LiveLine::Hello {
                    base: "0badf00d".into(),
                    probes: String::new(),
                },
            ),
            (
                "DEADTUNE_LIVE hello 285ebd44 ctl=1024 col=1024 d=1000,1001 n=12 gi=0 kv=1",
                LiveLine::Hello {
                    base: "285ebd44".into(),
                    probes: "ctl=1024 col=1024 d=1000,1001 n=12 gi=0 kv=1".into(),
                },
            ),
            (
                "DEADTUNE_LIVE 12 ok 0badf00d",
                LiveLine::Ok {
                    seq: 12,
                    base: "0badf00d".into(),
                },
            ),
            (
                "DEADTUNE_LIVE 12 wrongbase cafe0001",
                LiveLine::WrongBase {
                    seq: 12,
                    base: "cafe0001".into(),
                },
            ),
            (
                "DEADTUNE_LIVE 12 got 2 0badf00d",
                LiveLine::Got {
                    seq: 12,
                    have: 2,
                    base: "0badf00d".into(),
                },
            ),
        ];
        for (line, want) in cases {
            assert_eq!(parse_line(line), Some(want.clone()), "{line}");
            assert_eq!(
                parse_line(&format!("[Console] {line}\r")),
                Some(want),
                "{line}"
            );
        }
        for line in [
            "DEADTUNE_ACK 1 2",
            "DEADTUNE_LIVE",
            "DEADTUNE_LIVE x ok 0badf00d",
            "DEADTUNE_LIVE 3 maybe 0badf00d",
            "[InputService] exec: couldn't exec '{}cfg/deadtune_hud.cfg', unable to read file",
        ] {
            assert_eq!(parse_line(line), None, "{line}");
        }
    }

    #[test]
    fn write_cfg_writes_plain_lines_and_empties_the_file() {
        let dir = tempfile::tempdir().unwrap();
        write_cfg(dir.path(), &reset_cmds()[..2]).unwrap();
        let text = std::fs::read_to_string(dir.path().join(CFG_NAME)).unwrap();
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            [
                "tv_chattimelimit \"0.2\"",
                "tv_broadcast_spew_threshold \"0.1\"",
            ]
        );
        write_cfg(dir.path(), &[]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join(CFG_NAME)).unwrap(),
            ""
        );
    }

    #[test]
    fn base_id_ignores_the_live_flag_and_is_stable() {
        let mut l = element_layout(&[(
            crate::hud::ElementId::Chat,
            ElementEdit {
                opacity_pct: 40,
                ..ElementEdit::default()
            },
        )]);
        l.live = false;
        let id = base_id(&l).unwrap();
        assert_eq!(id.len(), 8);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        l.live = true;
        assert_eq!(base_id(&l).unwrap(), id);
        assert_eq!(base_id(&l.clone()).unwrap(), id);
        assert_ne!(base_id(&HudLayout::default()).unwrap(), id);
    }

    #[test]
    fn every_element_edit_is_live() {
        let edits = [
            ElementEdit {
                offset_x: 40,
                offset_y: -20,
                ..ElementEdit::default()
            },
            ElementEdit {
                scale_pct: 150,
                ..ElementEdit::default()
            },
            ElementEdit {
                opacity_pct: 30,
                ..ElementEdit::default()
            },
            ElementEdit {
                visibility: Visibility::Hidden,
                ..ElementEdit::default()
            },
            ElementEdit {
                visibility: Visibility::Shown,
                ..ElementEdit::default()
            },
        ];
        for spec in ELEMENTS {
            let parts: Vec<&str> = spec.selector.split(',').collect();
            for edit in &edits {
                let l = element_layout(&[(spec.id, edit.clone())]);
                let got = rules(&l).unwrap();
                let decls = layout::declarations(edit, spec);
                assert_eq!(
                    got.len(),
                    decls.len() * parts.len(),
                    "{:?} {edit:?}",
                    spec.id
                );
                for r in &got {
                    assert!(parts.contains(&r.selector.as_str()), "{r:?}");
                    assert!(
                        decls.iter().any(|(p, v)| *p == r.prop && *v == r.value),
                        "{r:?}"
                    );
                }
                assert!(coverage(&l).is_empty(), "{:?}", coverage(&l));
            }
        }
    }

    #[test]
    fn selectors_outside_the_script_grammar_are_not_live() {
        let css = "#a .b:not(.c) Label{opacity:0.5;}\
                   #a>#b{opacity:0.5;}\
                   #a:hover{opacity:0.5;}\
                   .x::after{opacity:0.5;}\
                   #a[foo]{opacity:0.5;}\
                   .cls{flow-children:down;}\
                   .cls{width:10px;opacity:0.4;}\
                   #only Tag{width:10px;}\
                   #one,.two #three{transform:none;}";
        let mut l = HudLayout::default();
        l.extra_css.insert(HUD_STYLE.into(), css.into());
        let got = rules(&l).unwrap();
        assert_eq!(
            got,
            [
                rule(".cls", "opacity", "0.4"),
                rule("#one", "transform", "none"),
                rule("#only Tag", "width", "10px"),
                rule(".two #three", "transform", "none"),
                rule("#a .b:not(.c) Label", "opacity", "0.5"),
            ],
            "ordered by specificity, then source order"
        );
        let cov = coverage(&l);
        assert_eq!(cov.len(), 1);
        assert_eq!(cov[0].feature, HudFeature::CustomCss);
        assert!(
            cov[0].why.starts_with("6 style changes need a restart"),
            "{}",
            cov[0].why
        );
    }

    #[test]
    fn later_rules_for_the_same_key_win() {
        let mut l = element_layout(&[(
            crate::hud::ElementId::Chat,
            ElementEdit {
                opacity_pct: 50,
                ..ElementEdit::default()
            },
        )]);
        l.extra_css
            .insert(HUD_STYLE.into(), "#Chat{opacity:0.2;}".into());
        assert_eq!(rules(&l).unwrap(), [rule("#Chat", "opacity", "0.2")]);
    }

    fn everything() -> Vec<HudLayout> {
        let mut out = Vec::new();
        let top = TopBarStyle {
            missing_opacity_pct: 10,
            missing_desaturate: true,
            missing_darken: true,
            dead: DeadLook::Grayscale,
            portrait_scale_pct: 130,
            portrait_gap_px: 24,
            ally_color: Some(Color([1, 2, 3, 255])),
            enemy_color: Some(Color([4, 5, 6, 255])),
            clock: Treatment::Hidden,
            soul_lead: Treatment::Compact,
            rejuv_charges: Treatment::Hidden,
            hide_kill_counts: true,
            hide_player_souls: true,
            show_levels: true,
            hide_ultimate: true,
            hide_health_bars: true,
            hide_streaks: true,
            spawn_timers: true,
            urn_lead: true,
            purchases: true,
        };
        out.push(HudLayout {
            top_bar: top,
            ..HudLayout::default()
        });
        for preset in TopBarPreset::ALL {
            out.push(HudLayout {
                top_bar: preset.style(),
                ..HudLayout::default()
            });
        }
        let mut minimap = HudLayout::default();
        minimap.minimap.map_opacity_pct = 40;
        minimap.minimap.minimal = true;
        minimap
            .minimap
            .marker_scale_pct
            .insert(MarkerGroup::EnemyHeroes, 150);
        minimap
            .minimap
            .marker_scale_pct
            .insert(MarkerGroup::Objectives, 60);
        minimap
            .minimap_colors
            .insert(IconId::EnemyHero, Color([0, 0xD5, 0xFF, 0x80]));
        minimap
            .minimap_colors
            .insert(IconId::EnemyObjective, Color([0, 0xD5, 0xFF, 255]));
        out.push(minimap);
        for preset in HealthPreset::ALL {
            out.push(HudLayout {
                health: preset.style(),
                ..HudLayout::default()
            });
        }
        for preset in StatsPreset::ALL {
            out.push(HudLayout {
                player_stats: preset.style(),
                ..HudLayout::default()
            });
        }
        let mut map = HudLayout::default();
        map.apples_tunnels.apples.on = true;
        map.apples_tunnels.tunnels.on = true;
        map.apples_tunnels.clear_switching = true;
        out.push(map);
        out.push(HudLayout {
            ingame: IngameSettings {
                wide_fov: true,
                performance: ["r_citadel_shadow_quality".to_string()].into(),
            },
            ..HudLayout::default()
        });
        let mut custom = HudLayout::default();
        custom
            .extra_css
            .insert(HUD_STYLE.into(), "#TopBar:hover{opacity:0.5;}".into());
        out.push(custom);
        out
    }

    #[test]
    fn generators_classify_without_panics() {
        for l in everything() {
            let got = rules(&l).unwrap();
            for r in &got {
                assert!(LIVE_PROPS.contains(&r.prop.as_str()), "{r:?}");
            }
            for n in coverage(&l) {
                assert!(l.features().contains(&n.feature), "{n:?}");
                assert!(!n.why.is_empty());
            }
            let d = dictionary(&l).unwrap();
            let m = msg(1, Kind::Full, got.clone());
            let back = decode(&encode(&m, &d), &d).unwrap();
            assert_eq!(back.rules, got, "every generator's rules survive the codec");
        }
        let mut all = HudLayout::default();
        for l in everything() {
            all.top_bar = if l.top_bar.has_extras() {
                l.top_bar
            } else {
                all.top_bar
            };
            if !l.apples_tunnels.is_vanilla() {
                all.apples_tunnels = l.apples_tunnels;
            }
            if !l.ingame.is_vanilla() {
                all.ingame = l.ingame;
            }
            all.extra_css.extend(l.extra_css);
        }
        all.icons.insert(
            "panorama/images/hud/minimap/objective_icon_psd.vtex_c".into(),
            crate::hud::icons::IconOverride::new(crate::hud::icons::Source::Game),
        );
        let named: Vec<HudFeature> = coverage(&all).iter().map(|n| n.feature).collect();
        assert_eq!(
            named,
            [
                HudFeature::TopBar,
                HudFeature::ApplesTunnels,
                HudFeature::IngameSettings,
                HudFeature::CustomCss,
                HudFeature::Images
            ]
        );
        let top = &coverage(&all)[0];
        assert!(top.why.contains("spawn timers"), "{}", top.why);
    }

    #[test]
    fn overrides_reset_dropped_keys_and_keep_baked_ones() {
        use crate::hud::ElementId;
        let baked = element_layout(&[
            (
                ElementId::HealthAndAmmo,
                ElementEdit {
                    scale_pct: 50,
                    opacity_pct: 50,
                    ..ElementEdit::default()
                },
            ),
            (
                ElementId::PassiveItems,
                ElementEdit {
                    visibility: Visibility::Shown,
                    ..ElementEdit::default()
                },
            ),
            (
                ElementId::Chat,
                ElementEdit {
                    offset_x: 10,
                    ..ElementEdit::default()
                },
            ),
        ]);
        let mut baked = baked;
        baked
            .extra_css
            .insert(HUD_STYLE.into(), "#Chat{width:300px;}".into());
        let desired = element_layout(&[(
            ElementId::Chat,
            ElementEdit {
                offset_x: 10,
                opacity_pct: 20,
                ..ElementEdit::default()
            },
        )]);
        let got = overrides(&desired, &baked).unwrap();
        let mut sorted = got.rules.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            [
                rule("#Chat", "opacity", "0.2"),
                rule("#Chat", "transform", "translateX(10px) translateY(0px)"),
                rule("#health_and_abilities_container", "opacity", "1"),
                rule("#health_and_abilities_container", "ui-scale", "120%"),
                rule("#hud_passive_items", "visibility", "collapse"),
            ]
        );
        assert_eq!(got.not_resettable, [rule("#Chat", "width", "300px")]);
        assert_eq!(
            overrides(&baked, &baked).unwrap().rules,
            rules(&baked).unwrap(),
            "back to what Apply baked"
        );
        assert_eq!(reset("#TopBar", "visibility").as_deref(), Some("visible"));
        assert_eq!(reset(".x", "ui-scale").as_deref(), Some("100%"));
        assert_eq!(
            reset("#hudActivePlayerStats", "ui-scale").as_deref(),
            Some("100%")
        );
        assert_eq!(reset("#X", "width"), None);
    }

    #[test]
    fn script_carries_the_base_and_compile_adds_it_with_the_slots() {
        let d = dict();
        let text = script("0badf00d", &d);
        assert!(
            text.starts_with(
                "var DT_LIVE = { base: \"0badf00d\", baseWords: [2989, 61453], ctl: \"DtLiveCtl\", ctlMax: 1048575, probe: \"DtLiveProbe\", data: [\"DtLiveD0\", "
            ),
            "{text}"
        );
        assert!(text.contains("dict: [\"^transform^translateX(\", "));
        assert!(text.contains(
            "cfg: \"deadtune_hud\", poll: 0.25, pull: 0.3, pullTimeout: 3, webPort: 47613 };\n(function () {"
        ));
        assert!(
            !text.contains("GetSettingString("),
            "no read API call, only the probe"
        );
        assert_eq!(js_string("a\"b\\c\u{e9}"), "\"a\\\"b\\\\c\\u00e9\"");
        assert!(
            TIMEOUT.as_secs_f64() > 2.0 * PULL_SECS + 2.0 * POLL_SECS,
            "a pull round trip fits in the timeout"
        );
        let mut l = element_layout(&[(
            crate::hud::ElementId::Chat,
            ElementEdit {
                opacity_pct: 40,
                ..ElementEdit::default()
            },
        )]);
        l.live = false;
        assert!(
            !layout::compile(&l)
                .unwrap()
                .own_files
                .contains_key(OWN_SCRIPT)
        );
        l.live = true;
        let patch = layout::compile(&l).unwrap();
        let edit = &patch.layouts[HUD_LAYOUT];
        assert_eq!(edit.script_includes, [format!("s2r://{OWN_SCRIPT}")]);
        assert_eq!(edit.panels.len(), 1);
        assert_eq!(edit.panels[0].0, Anchor::After("TopBar".into()));
        assert_eq!(edit.panels[0].1.id(), Some(SLOTS_ID));
        assert_eq!(
            patch.own_files[OWN_SCRIPT],
            script(&base_id(&l).unwrap(), &dictionary(&l).unwrap())
        );
        let alone = layout::compile(&HudLayout {
            live: true,
            ..HudLayout::default()
        })
        .unwrap();
        assert!(
            alone.is_empty(),
            "the switch alone ships nothing: Vanilla has no pak"
        );
    }

    fn pairs(cmds: &[ConsoleCmd]) -> Vec<(String, String)> {
        cmds.iter()
            .map(|c| (c.name.clone(), c.value.clone()))
            .collect()
    }

    /// The real script in `tests/fixtures/live_hud_sim.js`, a stand-in for Panorama. Skipped
    /// where Node is missing; the Mac that builds releases has it.
    #[test]
    fn the_script_reads_the_sliders_quietly_and_applies_a_pulled_message() {
        use serde_json::{Value, json};
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("node not found; skipping the script simulation");
            return;
        }
        let mut baked = HudLayout::default();
        for (i, e) in ELEMENTS.iter().enumerate() {
            baked.elements.insert(
                e.id,
                ElementEdit {
                    offset_x: 10 + i as i32,
                    offset_y: -3,
                    opacity_pct: 70,
                    ..ElementEdit::default()
                },
            );
        }
        let base = base_id(&baked).unwrap();
        let dict = dictionary(&baked).unwrap();
        let wanted = rules(&baked).unwrap();
        let seq = 77;
        let chunks = encode(
            &Message {
                seq,
                base: base.clone(),
                kind: Kind::Full,
                pull: true,
                rules: wanted.clone(),
            },
            &dict,
        );
        assert!(chunks.len() > 2, "{} chunks", chunks.len());
        let panels: BTreeSet<&str> = wanted
            .iter()
            .filter_map(|r| r.selector.strip_prefix('#'))
            .filter(|id| !id.contains([' ', '.', ':']))
            .collect();
        let scenario = json!({
            "script": script(&base, &dict),
            "exec": exec_line(),
            "ctl": { "id": CTL.id, "convar": CTL.convar },
            "data": DATA.iter().map(|s| json!({ "id": s.id, "convar": s.convar })).collect::<Vec<_>>(),
            "probe_id": PROBE_ID,
            "panels": panels,
            "boot": pairs(&probe_cmds()),
            "chunks": chunks
                .iter()
                .enumerate()
                .map(|(k, words)| pairs(&chunk_cmds(seq, k + 1, words)))
                .collect::<Vec<_>>(),
            "ping": [[CTL.convar, ((seq + 1) << 10).to_string()]],
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scenario.json");
        std::fs::write(&path, scenario.to_string()).unwrap();
        let sim = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/live_hud_sim.js"
        );
        let out = std::process::Command::new("node")
            .arg(sim)
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        let lines = |k: &str| -> Vec<String> {
            report[k]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        };
        let hello = format!(
            "echo DEADTUNE_LIVE hello {base} ctl=1024 raw=1.024/0 col=1024 d=1000,1001,1002,1003,1004,1005,1006,1007,1008,1009 n=10 api=missing"
        );
        let start = lines("start");
        assert_eq!(start.len(), 1, "{start:?}");
        assert!(start[0].starts_with(&hello), "{}", start[0]);
        assert_eq!(
            lines("idle"),
            Vec::<String>::new(),
            "an idle HUD prints nothing"
        );
        let message = lines("message");
        let execs = message.iter().filter(|l| **l == exec_line()).count();
        let n = chunks.len();
        assert!(
            (n - 1..=3 * n).contains(&execs),
            "the script pulls each further chunk: {message:?}"
        );
        assert_eq!(
            message.last().unwrap(),
            &format!("echo DEADTUNE_LIVE {seq} ok {base}"),
            "{message:?}"
        );
        for k in 1..n {
            assert!(
                message.contains(&format!("echo DEADTUNE_LIVE {seq} got {k} {base}")),
                "{message:?}"
            );
        }
        for r in &wanted {
            let Some(id) = r
                .selector
                .strip_prefix('#')
                .filter(|id| panels.contains(id))
            else {
                continue;
            };
            let prop = r
                .prop
                .split('-')
                .enumerate()
                .map(|(i, w)| {
                    if i == 0 {
                        w.to_string()
                    } else {
                        w[..1].to_uppercase() + &w[1..]
                    }
                })
                .collect::<String>();
            assert_eq!(
                report["styles"][id][&prop].as_str(),
                Some(r.value.as_str()),
                "{id} {prop}"
            );
        }
        assert_eq!(
            lines("after"),
            Vec::<String>::new(),
            "quiet again after the message"
        );
        let ping = lines("ping");
        assert_eq!(ping.len(), 1, "{ping:?}");
        assert!(
            ping[0].contains(&format!("hello {base} ctl={}", (seq + 1) << 10)),
            "{}",
            ping[0]
        );
    }

    #[test]
    fn the_stand_in_layout_is_a_compiled_layout_the_pipeline_patches() {
        let compiled = inject::compiled_layout(&stand_in_layout());
        let l = moved(10);
        let patch = layout::compile(&l).unwrap();
        let built =
            inject::patched_layout(&compiled, &patch.layouts[HUD_LAYOUT], &[], "test").unwrap();
        let text = inject::layout_text(&built).unwrap();
        assert!(inject::extends(
            &text,
            &inject::layout_text(&compiled).unwrap()
        ));
        assert!(
            text.contains("<include src=\"s2r://panorama/scripts/deadtune/live_hud.vjs_c\" />"),
            "{text}"
        );
        let top = text.find("id=\"TopBar\"").unwrap();
        let slots = text.find("id=\"DtLive\"").unwrap();
        let map = text.find("id=\"minimap_persp\"").unwrap();
        assert!(
            top < slots && slots < map,
            "the slots sit right after the top bar"
        );
        assert!(text.contains(
            "<CitadelSettingsSlider id=\"DtLiveCtl\" class=\"VideoPreview\" convar=\"tv_chattimelimit\" min=\"0\" max=\"1048575\" snap=\"1\" percentage=\"false\" displayprecision=\"0\" text=\"DtLiveCtl\" textentry=\"true\" />"
        ), "{text}");
        assert!(text.contains("convar=\"tv_timeout\" min=\"0\" max=\"65535\""));
        assert!(text.contains("id=\"DtLiveProbe\""));
        assert!(text.contains("visibility: collapse;"));
        let resource = inject::script_resource(&patch.own_files[OWN_SCRIPT]);
        let parsed = crate::hud::resource::Resource::parse(&resource).unwrap();
        assert!(
            parsed.blocks[0]
                .data
                .starts_with(b"var DT_LIVE = { base: \"")
        );
    }
}
