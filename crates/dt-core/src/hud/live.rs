//! Live HUD preview: restyles the running game's HUD without a restart.
//!
//! The running game locks the HUD pak and Panorama has no reload, so the pak ships one
//! script of ours (`assets/live_hud.js`) that sets `panel.style.<prop>` from rules DeadTune
//! hands it through three harmless string ConVars (`SLOTS`). The script `exec`s
//! `CFG_NAME` on every poll, so DeadTune only writes that file (or sends the same lines over
//! netcon). Every message carries the `base` of the pak the game runs, so overrides are
//! always relative to what the pak baked. Design: docs/plans/live-hud/plan.md.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::css;
use super::elements::ELEMENTS;
use super::inject::{Element, LayoutEdit};
use super::layout::{self, HudFeature, HudLayout, HudPatch, LayoutError};
use crate::bridge::{BridgeError, ConsoleCmd, execfile};

pub const LIVE: &str = "DEADTUNE_LIVE";
pub const SLOTS: [&str; 3] = ["iv_debugbone", "tv_title", "tv_name"];
const SLOT_DEFAULTS: [&str; 3] = ["", "SourceTV", "SourceTV"];
/// Payload characters per chunk; the header comes on top.
pub const CHUNK: usize = 200;
/// How long a batch may wait for the script's answer before everything is sent again.
/// Longer than an idle script takes to read the cfg, plus the console log's delay.
pub const TIMEOUT: Duration = Duration::from_secs(2);
/// The script reads the slots this often, and execs the cfg this often for `HOT_SECS`
/// after a chunk arrives.
pub const POLL_SECS: f64 = 0.25;
/// How often an idle script execs the cfg, since the console may log every exec.
pub const IDLE_SECS: f64 = 1.0;
pub const HOT_SECS: f64 = 10.0;
pub const HELLO_SECS: f64 = 10.0;
pub const CFG_NAME: &str = "deadtune_hud.cfg";
pub const OWN_SCRIPT: &str = "panorama/scripts/deadtune/live_hud.vjs_c";
/// The HUD root layout; our script goes into its `<scripts>`.
pub const HUD_LAYOUT: &str = "panorama/layout/hud.vxml_c";
const SCRIPT: &str = include_str!("assets/live_hud.js");
const FORMAT: &str = "dt1";

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

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Full => "full",
            Kind::Patch => "patch",
        }
    }

    fn parse(word: &str) -> Option<Kind> {
        [Kind::Full, Kind::Patch]
            .into_iter()
            .find(|k| k.word() == word)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub seq: u32,
    pub base: String,
    pub kind: Kind,
    pub rules: Vec<LiveRule>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("no chunks")]
    Empty,
    #[error("not a live HUD chunk: {0}")]
    Header(String),
    #[error("chunks from different messages")]
    Mixed,
    #[error("chunk {0} of {1} is missing")]
    Missing(usize, usize),
    #[error("bad record: {0}")]
    Record(String),
}

/// What the script echoes into the console log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveLine {
    Hello {
        base: String,
        /// The first 24 characters the script read from `tv_title`, for probe LH-P1;
        /// `None` from a script that does not report it.
        read: Option<String>,
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

/// Eight hex digits naming the rules a pak bakes, the same with the live switch on or off.
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
    hash.finalize()[..4]
        .iter()
        .fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// Kept as is in a field; everything else is `%XX` per UTF-8 byte, so a chunk has no
/// space the console could trim, no `"`, `;` or line break it refuses, and none of the
/// record separators `~` and `^`.
fn plain(c: char) -> bool {
    c.is_ascii_alphanumeric() || "#.-_(),:!*+=".contains(c)
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

/// The message as ConVar values: `dt1 <seq> <i>/<n> <base> <kind> <payload>`, records
/// `selector^prop^value` joined by `~`, at most `CHUNK` payload characters each.
pub fn encode(msg: &Message) -> Vec<String> {
    let payload = msg
        .rules
        .iter()
        .map(|r| {
            [&r.selector, &r.prop, &r.value]
                .map(|f| escape(f))
                .join("^")
        })
        .collect::<Vec<_>>()
        .join("~");
    let pieces: Vec<&str> = if payload.is_empty() {
        vec![""]
    } else {
        payload
            .as_bytes()
            .chunks(CHUNK)
            .map(|c| std::str::from_utf8(c).expect("escaped payloads are ASCII"))
            .collect()
    };
    let n = pieces.len();
    pieces
        .iter()
        .enumerate()
        .map(|(i, piece)| {
            format!(
                "{FORMAT} {} {}/{n} {} {} {piece}",
                msg.seq,
                i + 1,
                msg.base,
                msg.kind.word()
            )
        })
        .collect()
}

struct Chunk<'a> {
    seq: u32,
    i: usize,
    n: usize,
    base: &'a str,
    kind: Kind,
    payload: &'a str,
}

fn parse_chunk(text: &str) -> Option<Chunk<'_>> {
    let mut parts = text.splitn(6, ' ');
    if parts.next()? != FORMAT {
        return None;
    }
    let seq = parts.next()?.parse().ok()?;
    let (i, n) = parts.next()?.split_once('/')?;
    let (i, n): (usize, usize) = (i.parse().ok()?, n.parse().ok()?);
    let base = parts.next()?;
    let kind = Kind::parse(parts.next()?)?;
    let payload = parts.next().unwrap_or("");
    (1..=n).contains(&i).then_some(Chunk {
        seq,
        i,
        n,
        base,
        kind,
        payload,
    })
}

/// The message the chunks carry, in any order; what the script does in game.
pub fn decode(chunks: &[String]) -> Result<Message, CodecError> {
    let parsed: Vec<Chunk> = chunks
        .iter()
        .map(|c| parse_chunk(c).ok_or_else(|| CodecError::Header(c.clone())))
        .collect::<Result<_, _>>()?;
    let first = parsed.first().ok_or(CodecError::Empty)?;
    let mut pieces = BTreeMap::new();
    for c in &parsed {
        if (c.seq, c.n, c.base, c.kind) != (first.seq, first.n, first.base, first.kind) {
            return Err(CodecError::Mixed);
        }
        pieces.insert(c.i, c.payload);
    }
    if let Some(i) = (1..=first.n).find(|i| !pieces.contains_key(i)) {
        return Err(CodecError::Missing(i, first.n));
    }
    let payload: String = pieces.into_values().collect();
    let rules = if payload.is_empty() {
        Vec::new()
    } else {
        payload
            .split('~')
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
            .collect::<Result<_, _>>()?
    };
    Ok(Message {
        seq: first.seq,
        base: first.base.to_string(),
        kind: first.kind,
        rules,
    })
}

/// One batch of at most `SLOTS.len()` chunks as slot settings, in slot order; unused slots
/// are cleared so no chunk of an earlier batch lingers.
pub fn cmds(chunks: &[String]) -> Vec<ConsoleCmd> {
    SLOTS
        .iter()
        .enumerate()
        .map(|(i, name)| ConsoleCmd {
            name: name.to_string(),
            value: chunks.get(i).cloned().unwrap_or_default(),
        })
        .collect()
}

/// Every slot back to the game's default, for the end of a session.
pub fn reset_cmds() -> Vec<ConsoleCmd> {
    SLOTS
        .iter()
        .zip(SLOT_DEFAULTS)
        .map(|(name, value)| ConsoleCmd {
            name: name.to_string(),
            value: value.to_string(),
        })
        .collect()
}

/// A line the script echoed, tolerant of whatever the log puts before the marker.
pub fn parse_line(line: &str) -> Option<LiveLine> {
    let at = line.find(LIVE)?;
    let mut words = line[at + LIVE.len()..].split_whitespace();
    let first = words.next()?;
    if first == "hello" {
        return Some(LiveLine::Hello {
            base: words.next()?.to_string(),
            read: words
                .next()
                .and_then(|w| w.strip_prefix("tv_title="))
                .and_then(unescape),
        });
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

/// Writes `cmds` as the file the script execs on every poll; no commands empties it.
pub fn write_cfg(cfg_dir: &Path, cmds: &[ConsoleCmd]) -> Result<(), BridgeError> {
    let lines: Vec<String> = cmds
        .iter()
        .map(ConsoleCmd::to_line)
        .collect::<Result<_, _>>()?;
    Ok(execfile::write_cfg(cfg_dir, CFG_NAME, &lines)?)
}

/// Our script for a pak whose rules hash to `base`.
pub fn script(base: &str) -> String {
    let slots = SLOTS.map(|s| format!("\"{s}\"")).join(", ");
    let cfg = CFG_NAME.trim_end_matches(".cfg");
    format!(
        "var DT_LIVE = {{ base: \"{base}\", slots: [{slots}], cfg: \"{cfg}\", poll: {POLL_SECS}, idle: {IDLE_SECS}, hot: {HOT_SECS}, hello: {HELLO_SECS} }};\n{SCRIPT}"
    )
}

/// Adds the script to a patch compiled without it.
pub(crate) fn add_to(patch: &mut HudPatch) {
    let base = patch_base(patch);
    let edit: &mut LayoutEdit = patch.layouts.entry(HUD_LAYOUT.to_string()).or_default();
    edit.script_includes.push(format!("s2r://{OWN_SCRIPT}"));
    patch
        .own_files
        .insert(OWN_SCRIPT.to_string(), script(&base));
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

/// What the script still has to apply: everything after a `Full`, or the keys patched
/// since the last message.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Dirty {
    Full,
    Keys(BTreeSet<(String, String)>),
}

#[derive(Clone, Debug)]
struct Flight {
    seq: u32,
    chunks: Vec<String>,
    /// Chunks handed out so far.
    sent: usize,
    /// Chunks the script reported holding.
    received: usize,
    at: Instant,
}

/// One message in flight at a time, cut into batches of `SLOTS.len()` chunks. It keeps the
/// override set the game should show, so a lost message is made good by sending everything
/// again as `Full`. Pure: the caller delivers the commands and feeds back the script's
/// lines.
#[derive(Clone, Debug)]
pub struct Mailbox {
    base: String,
    next_seq: u32,
    desired: Vec<LiveRule>,
    dirty: Option<Dirty>,
    flight: Option<Flight>,
}

impl Mailbox {
    /// `first_seq` should differ between DeadTune runs (seed it from the clock): the script
    /// ignores a message whose seq it applied last.
    pub fn new(base: &str, first_seq: u32) -> Mailbox {
        Mailbox {
            base: base.to_string(),
            next_seq: first_seq,
            desired: Vec::new(),
            dirty: None,
            flight: None,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// Records the rules and returns the first batch when nothing is in flight.
    pub fn send(
        &mut self,
        rules: Vec<LiveRule>,
        kind: Kind,
        now: Instant,
    ) -> Option<Vec<ConsoleCmd>> {
        match kind {
            Kind::Full => {
                self.desired = rules;
                self.dirty = Some(Dirty::Full);
            }
            Kind::Patch => {
                let mut keys = match self.dirty.take() {
                    Some(Dirty::Keys(keys)) => Some(keys),
                    Some(Dirty::Full) => None,
                    None => Some(BTreeSet::new()),
                };
                for rule in rules {
                    if let Some(keys) = keys.as_mut() {
                        keys.insert(rule.key());
                    }
                    match self.desired.iter_mut().find(|r| r.key() == rule.key()) {
                        Some(slot) => *slot = rule,
                        None => self.desired.push(rule),
                    }
                }
                self.dirty = Some(keys.map_or(Dirty::Full, Dirty::Keys));
            }
        }
        self.poll(now)
    }

    /// The next batch to deliver, if any: the rest of the message in flight once the
    /// script holds what was sent, or the next message once it is acked. A batch left
    /// unanswered for `TIMEOUT` makes the next message `Full`.
    pub fn poll(&mut self, now: Instant) -> Option<Vec<ConsoleCmd>> {
        if let Some(flight) = &mut self.flight {
            if now.duration_since(flight.at) < TIMEOUT {
                if flight.received < flight.sent || flight.sent == flight.chunks.len() {
                    return None;
                }
                let end = (flight.sent + SLOTS.len()).min(flight.chunks.len());
                let batch = cmds(&flight.chunks[flight.sent..end]);
                flight.sent = end;
                flight.at = now;
                return Some(batch);
            }
            self.flight = None;
            self.dirty = Some(Dirty::Full);
        }
        let (kind, rules) = match self.dirty.take()? {
            Dirty::Full => (Kind::Full, self.desired.clone()),
            Dirty::Keys(keys) => (
                Kind::Patch,
                self.desired
                    .iter()
                    .filter(|r| keys.contains(&r.key()))
                    .cloned()
                    .collect(),
            ),
        };
        let seq = self.next_seq;
        self.next_seq = seq.wrapping_add(1);
        let chunks = encode(&Message {
            seq,
            base: self.base.clone(),
            kind,
            rules,
        });
        let sent = chunks.len().min(SLOTS.len());
        let batch = cmds(&chunks[..sent]);
        self.flight = Some(Flight {
            seq,
            chunks,
            sent,
            received: 0,
            at: now,
        });
        Some(batch)
    }

    /// The script applied message `seq`.
    pub fn ack(&mut self, seq: u32) {
        if self.flight.as_ref().is_some_and(|f| f.seq == seq) {
            self.flight = None;
        }
    }

    /// The script holds `have` chunks of message `seq`.
    pub fn got(&mut self, seq: u32, have: usize) {
        if let Some(flight) = self.flight.as_mut().filter(|f| f.seq == seq) {
            flight.received = flight.received.max(have);
        }
    }

    /// A message is in flight or waiting to go.
    pub fn pending(&self) -> bool {
        self.flight.is_some() || self.dirty.is_some()
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

    fn values(cmds: &[ConsoleCmd]) -> Vec<String> {
        cmds.iter().map(|c| c.value.clone()).collect()
    }

    #[test]
    fn codec_round_trips_every_escaped_character() {
        let nasty = rule(
            "#a .b:not(.c)",
            "font-family",
            "100% ~ ^ \" ; \r\n %25 ü {x} // end ",
        );
        let m = msg(7, Kind::Patch, vec![nasty, rule("#X", "opacity", "")]);
        let chunks = encode(&m);
        assert_eq!(chunks.len(), 1);
        let payload = chunks[0].splitn(6, ' ').nth(5).unwrap();
        assert_eq!(payload.matches('~').count(), 1, "{payload}");
        assert_eq!(payload.matches('^').count(), 4, "{payload}");
        for c in ['"', ';', '\r', '\n', 'ü', '{', '/', ' '] {
            assert!(!payload.contains(c), "{c:?} in {payload}");
        }
        assert_eq!(decode(&chunks), Ok(m));
        let empty = msg(8, Kind::Full, Vec::new());
        let chunks = encode(&empty);
        assert_eq!(chunks, ["dt1 8 1/1 0badf00d full "]);
        assert_eq!(decode(&chunks), Ok(empty.clone()));
        assert_eq!(
            decode(&[chunks[0].trim_end().to_string()]),
            Ok(empty),
            "the console may trim the trailing space"
        );
    }

    #[test]
    fn long_messages_split_into_chunks_that_reassemble_in_any_order() {
        let m = msg(41, Kind::Full, many(30));
        let mut chunks = encode(&m);
        assert!(chunks.len() > SLOTS.len(), "{}", chunks.len());
        assert!(chunks[0].starts_with(&format!("dt1 41 1/{} 0badf00d full ", chunks.len())));
        chunks.reverse();
        assert_eq!(decode(&chunks), Ok(m));
    }

    #[test]
    fn chunks_stay_within_the_limit_and_the_console_accepts_them() {
        let m = msg(u32::MAX, Kind::Patch, many(30));
        for chunk in encode(&m) {
            let header = chunk.splitn(6, ' ').take(5).collect::<Vec<_>>().join(" ");
            assert!(chunk.len() <= header.len() + 1 + CHUNK, "{chunk}");
            assert!(!chunk[header.len() + 1..].contains(' '), "{chunk}");
        }
        for cmd in cmds(&encode(&msg(1, Kind::Full, many(5)))[..1]) {
            cmd.to_line().expect("the console takes every chunk");
        }
    }

    #[test]
    fn decode_refuses_mixed_missing_and_foreign_chunks() {
        let a = encode(&msg(1, Kind::Full, many(30)));
        let b = encode(&msg(2, Kind::Full, many(30)));
        assert_eq!(decode(&[]), Err(CodecError::Empty));
        assert_eq!(
            decode(&[a[0].clone(), b[1].clone()]),
            Err(CodecError::Mixed)
        );
        assert_eq!(
            decode(&a[1..]),
            Err(CodecError::Missing(1, a.len())),
            "the first chunk is missing"
        );
        assert!(matches!(
            decode(&["hello there".to_string()]),
            Err(CodecError::Header(_))
        ));
        assert!(matches!(
            decode(&["dt1 1 1/1 0badf00d full a^b".to_string()]),
            Err(CodecError::Record(_))
        ));
    }

    #[test]
    fn cmds_put_one_chunk_in_each_slot_and_clear_the_rest() {
        let chunks: Vec<String> = ["a", "b", "c"].map(String::from).into();
        let got = cmds(&chunks);
        assert_eq!(
            got.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            SLOTS
        );
        assert_eq!(values(&got), ["a", "b", "c"]);
        assert_eq!(values(&cmds(&chunks[..1])), ["a", "", ""]);
        let resets = reset_cmds();
        assert_eq!(
            resets
                .iter()
                .map(|c| c.to_line().unwrap())
                .collect::<Vec<_>>(),
            [
                "iv_debugbone \"\"",
                "tv_title \"SourceTV\"",
                "tv_name \"SourceTV\""
            ]
        );
    }

    #[test]
    fn mailbox_keeps_one_message_in_flight_until_the_ack() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut mb = Mailbox::new("0badf00d", 5);
        assert!(!mb.pending());
        let first = mb
            .send(vec![rule("#TopBar", "opacity", "0.5")], Kind::Full, t0)
            .unwrap();
        assert_eq!(
            decode(&values(&first)[..1]).unwrap(),
            msg(5, Kind::Full, vec![rule("#TopBar", "opacity", "0.5")])
        );
        assert!(mb.pending());
        assert_eq!(
            mb.send(vec![rule("#Chat", "opacity", "0")], Kind::Patch, ms(10)),
            None,
            "one message in flight"
        );
        assert_eq!(mb.poll(ms(500)), None);
        mb.ack(4);
        assert_eq!(mb.poll(ms(600)), None, "an old seq does not free the slot");
        mb.ack(5);
        let second = mb.poll(ms(700)).unwrap();
        assert_eq!(
            decode(&values(&second)[..1]).unwrap(),
            msg(6, Kind::Patch, vec![rule("#Chat", "opacity", "0")])
        );
        mb.ack(6);
        assert_eq!(mb.poll(ms(800)), None);
        assert!(!mb.pending());
    }

    #[test]
    fn mailbox_patches_merge_while_one_is_in_flight() {
        let t0 = Instant::now();
        let mut mb = Mailbox::new("0badf00d", 1);
        mb.send(vec![rule("#A", "opacity", "1")], Kind::Full, t0);
        mb.send(vec![rule("#B", "opacity", "0.1")], Kind::Patch, t0);
        mb.send(
            vec![rule("#B", "opacity", "0.2"), rule("#C", "opacity", "0.3")],
            Kind::Patch,
            t0,
        );
        mb.ack(1);
        let next = mb.poll(t0).unwrap();
        assert_eq!(
            decode(&values(&next)[..1]).unwrap(),
            msg(
                2,
                Kind::Patch,
                vec![rule("#B", "opacity", "0.2"), rule("#C", "opacity", "0.3")]
            )
        );
    }

    #[test]
    fn mailbox_sends_long_messages_batch_by_batch() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut mb = Mailbox::new("0badf00d", 9);
        let all = encode(&msg(9, Kind::Full, many(30)));
        let first = mb.send(many(30), Kind::Full, t0).unwrap();
        assert_eq!(values(&first), all[..3]);
        assert_eq!(mb.poll(ms(100)), None);
        mb.got(9, 2);
        assert_eq!(mb.poll(ms(200)), None, "the batch is not all in yet");
        mb.got(9, 3);
        let second = mb.poll(ms(300)).unwrap();
        let mut want: Vec<String> = all[3..all.len().min(6)].to_vec();
        want.resize(3, String::new());
        assert_eq!(values(&second), want);
        assert_eq!(
            mb.poll(t0 + TIMEOUT + Duration::from_millis(100)),
            None,
            "the timeout counts from the latest batch"
        );
    }

    #[test]
    fn mailbox_timeout_sends_everything_again_as_full() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut mb = Mailbox::new("0badf00d", 1);
        mb.send(vec![rule("#A", "opacity", "1")], Kind::Full, t0);
        mb.ack(1);
        mb.send(vec![rule("#B", "opacity", "0")], Kind::Patch, ms(10));
        assert_eq!(mb.poll(ms(10) + TIMEOUT - Duration::from_millis(1)), None);
        let again = mb.poll(ms(10) + TIMEOUT).unwrap();
        assert_eq!(
            decode(&values(&again)[..1]).unwrap(),
            msg(
                3,
                Kind::Full,
                vec![rule("#A", "opacity", "1"), rule("#B", "opacity", "0")]
            )
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
    fn parse_line_reads_every_live_line() {
        let cases = [
            (
                "DEADTUNE_LIVE hello 0badf00d",
                LiveLine::Hello {
                    base: "0badf00d".into(),
                    read: None,
                },
            ),
            (
                "DEADTUNE_LIVE hello 0badf00d tv_title=dt1%201%201/1%20x",
                LiveLine::Hello {
                    base: "0badf00d".into(),
                    read: Some("dt1 1 1/1 x".into()),
                },
            ),
            (
                "DEADTUNE_LIVE hello 0badf00d tv_title=dt1%205%201%2F1%20x%3B%20y%22z%20%25%20and%20a",
                LiveLine::Hello {
                    base: "0badf00d".into(),
                    read: Some("dt1 5 1/1 x; y\"z % and a".into()),
                },
            ),
            (
                "DEADTUNE_LIVE hello 0badf00d tv_title=",
                LiveLine::Hello {
                    base: "0badf00d".into(),
                    read: Some(String::new()),
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
                parse_line(&format!("[Console] {line}")),
                Some(want),
                "{line}"
            );
        }
        for line in [
            "DEADTUNE_ACK 1 2",
            "DEADTUNE_LIVE",
            "DEADTUNE_LIVE x ok 0badf00d",
            "DEADTUNE_LIVE 3 maybe 0badf00d",
        ] {
            assert_eq!(parse_line(line), None, "{line}");
        }
    }

    #[test]
    fn write_cfg_writes_plain_lines_and_empties_the_file() {
        let dir = tempfile::tempdir().unwrap();
        write_cfg(dir.path(), &reset_cmds()).unwrap();
        let text = std::fs::read_to_string(dir.path().join(CFG_NAME)).unwrap();
        assert_eq!(
            text.lines().collect::<Vec<_>>(),
            [
                "iv_debugbone \"\"",
                "tv_title \"SourceTV\"",
                "tv_name \"SourceTV\""
            ]
        );
        write_cfg(dir.path(), &[]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join(CFG_NAME)).unwrap(),
            ""
        );
    }

    fn element_layout(edits: &[(crate::hud::ElementId, ElementEdit)]) -> HudLayout {
        HudLayout {
            elements: edits.iter().cloned().collect(),
            ..HudLayout::default()
        }
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
    fn script_carries_the_base_and_compile_adds_it() {
        let text = script("0badf00d");
        assert!(text.starts_with("var DT_LIVE = { base: \"0badf00d\", slots: [\"iv_debugbone\", \"tv_title\", \"tv_name\"], cfg: \"deadtune_hud\", poll: 0.25, idle: 1, hot: 10, hello: 10 };\n(function () {"), "{text}");
        assert!(text.contains("GetSettingString"));
        assert!(
            TIMEOUT.as_secs_f64() > IDLE_SECS + POLL_SECS,
            "an idle script reads the cfg before the mailbox gives up"
        );
        let mut l = element_layout(&[(
            crate::hud::ElementId::Chat,
            ElementEdit {
                opacity_pct: 40,
                ..ElementEdit::default()
            },
        )]);
        assert!(
            !layout::compile(&l)
                .unwrap()
                .own_files
                .contains_key(OWN_SCRIPT)
        );
        l.live = true;
        let patch = layout::compile(&l).unwrap();
        assert_eq!(
            patch.layouts[HUD_LAYOUT].script_includes,
            [format!("s2r://{OWN_SCRIPT}")]
        );
        assert_eq!(patch.own_files[OWN_SCRIPT], script(&base_id(&l).unwrap()));
        let alone = layout::compile(&HudLayout {
            live: true,
            ..HudLayout::default()
        })
        .unwrap();
        assert!(!alone.is_empty(), "the switch alone ships the script");
    }

    #[test]
    fn the_stand_in_layout_is_a_compiled_layout_the_pipeline_patches() {
        let compiled = inject::compiled_layout(&stand_in_layout());
        let l = HudLayout {
            live: true,
            ..HudLayout::default()
        };
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
        let resource = inject::script_resource(&patch.own_files[OWN_SCRIPT]);
        let parsed = crate::hud::resource::Resource::parse(&resource).unwrap();
        assert!(
            parsed.blocks[0]
                .data
                .starts_with(b"var DT_LIVE = { base: \"")
        );
    }
}
