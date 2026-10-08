//! Live HUD preview: restyles the running game's HUD without a restart.
//!
//! The running game locks the HUD pak and Panorama has no reload, so the pak ships one
//! script of ours (`assets/live_hud.js`) that sets `panel.style.<prop>` from rules DeadTune
//! hands it. Panorama JS can read neither a ConVar nor a file, so the script opens a hidden
//! web panel on DeadTune's bridge page (`docs/bridge/`, served by GitHub Pages), which
//! fetches each message from DeadTune on 127.0.0.1 (`web_bridge`) and hands it over as
//! title changes. Every message carries the `base` of the pak the game runs, so overrides
//! are always relative to what the pak baked. Design: docs/plans/live-hud/plan.md.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::css;
use super::elements::ELEMENTS;
use super::inject::{Element, LayoutEdit};
use super::layout::{self, HudFeature, HudLayout, HudPatch, LayoutError};
use super::web_bridge;

pub const LIVE: &str = "DEADTUNE_LIVE";
/// First word of every message chunk the page hands the script.
const FORMAT: &str = "dt1";
/// Payload characters per chunk; the page sets one title per chunk. QOL Lock sends 1500 per
/// title in this game; fewer titles means fewer to lose at a low frame rate.
pub const CHUNK_CHARS: usize = 1500;
/// How long a message may wait for the script's ack before it goes again as a full one.
pub const TIMEOUT: Duration = Duration::from_secs(3);
/// The script restyles when a message arrives. Rules keyed on a class (minimap markers,
/// a dead hero's portrait) also need re-matching as panels come and go: this often while
/// someone edits the HUD (awake), and `SLEEP_BEAT_SECS` while the bridge sleeps. Rules on
/// ids alone are never re-matched.
pub const BEAT_SECS: f64 = 1.0;
pub const SLEEP_BEAT_SECS: f64 = 3.0;
/// A message still missing chunks this long after its first one asks the page for them.
pub const NEED_SECS: f64 = 0.5;
/// While awake the page sets a title at least every `web_bridge::WAIT`; a script that
/// heard nothing for this long loads the page again.
pub const QUIET_SECS: f64 = 30.0;
/// The script opens the bridge page again when it hasn't said ready after this long, up to
/// `RETRIES` loads in all.
pub const RETRY_SECS: f64 = 20.0;
pub const RETRIES: u32 = 5;
pub const OWN_SCRIPT: &str = "panorama/scripts/deadtune/live_hud.vjs_c";
/// The HUD root layout; our script goes into its `<scripts>`.
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
    pub rules: Vec<LiveRule>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("no chunks")]
    Empty,
    #[error("chunk {0} of {1} is missing")]
    Missing(usize, usize),
    #[error("bad chunk: {0}")]
    Chunk(String),
    #[error("bad record: {0}")]
    Record(String),
}

/// What the script writes into the console log.
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
    /// What the web panel reported: `ready storage=ok`, `fetch ok`, `fetch blocked <why>`,
    /// `retry <n>`, `gave up`, `reload <n>`, `nopanel`, ...
    Web(String),
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

/// Kept as is in a field; `%`, the record separators `~` and `^`, spaces (a page title
/// collapses them), control characters and anything outside ASCII are `%XX` per UTF-8
/// byte, so the payload is printable ASCII without spaces.
fn plain(c: char) -> bool {
    c.is_ascii_graphic() && !"%~^".contains(c)
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

fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Full => "full",
        Kind::Patch => "patch",
    }
}

/// The message as the chunks the bridge page sets as titles, in order:
/// `dt1 <seq> <i>/<n> <base> full|patch <payload part>`. The payload has no spaces, so it
/// splits anywhere; the script joins the parts before decoding them.
pub fn titles(msg: &Message) -> Vec<String> {
    let chars: Vec<char> = payload(&msg.rules).chars().collect();
    let parts: Vec<String> = if chars.is_empty() {
        vec![String::new()]
    } else {
        chars
            .chunks(CHUNK_CHARS)
            .map(|c| c.iter().collect())
            .collect()
    };
    let n = parts.len();
    let kind = kind_word(msg.kind);
    parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            format!(
                "{FORMAT} {} {}/{n} {} {kind} {part}",
                msg.seq,
                i + 1,
                msg.base
            )
        })
        .collect()
}

/// One chunk read back: seq, part number, part count, base, kind and the payload part.
fn parse_title(title: &str) -> Option<(u32, usize, usize, String, Kind, String)> {
    let mut w = title.trim().splitn(6, ' ');
    if w.next()? != FORMAT {
        return None;
    }
    let seq = w.next()?.parse().ok()?;
    let (i, n) = w.next()?.split_once('/')?;
    let (i, n): (usize, usize) = (i.parse().ok()?, n.parse().ok()?);
    if !(1..=n).contains(&i) {
        return None;
    }
    let base = w.next()?.to_string();
    let kind = match w.next()? {
        "full" => Kind::Full,
        "patch" => Kind::Patch,
        _ => return None,
    };
    Some((seq, i, n, base, kind, w.next().unwrap_or("").to_string()))
}

/// The message `titles` carry, in any order; what the script does in game. A chunk may
/// have lost its trailing space, as the browser trims titles.
pub fn parse_titles(titles: &[String]) -> Result<Message, CodecError> {
    let mut head: Option<(u32, String, Kind)> = None;
    let mut parts: Vec<Option<String>> = Vec::new();
    for title in titles {
        let bad = || CodecError::Chunk(title.clone());
        let (seq, i, n, base, kind, part) = parse_title(title).ok_or_else(bad)?;
        match &head {
            None => {
                head = Some((seq, base, kind));
                parts = vec![None; n];
            }
            Some(h) if *h != (seq, base, kind) || parts.len() != n => return Err(bad()),
            Some(_) => {}
        }
        parts[i - 1] = Some(part);
    }
    let (seq, base, kind) = head.ok_or(CodecError::Empty)?;
    let n = parts.len();
    let text = parts
        .into_iter()
        .enumerate()
        .map(|(i, p)| p.ok_or(CodecError::Missing(i + 1, n)))
        .collect::<Result<String, _>>()?;
    Ok(Message {
        seq,
        base,
        kind,
        rules: parse_payload(&text)?,
    })
}

/// A line the script wrote, tolerant of whatever the log puts before the marker.
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
    if first == "web" {
        let text = rest["web".len()..].trim();
        return (!text.is_empty()).then(|| LiveLine::Web(text.to_string()));
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
        _ => None,
    }
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

/// Our script for a pak whose rules hash to `base`.
pub fn script(base: &str) -> String {
    format!(
        "var DT_LIVE = {{ base: \"{base}\", page: {}, port: {}, protocol: {}, beat: {BEAT_SECS}, sleepBeat: {SLEEP_BEAT_SECS}, need: {NEED_SECS}, quiet: {QUIET_SECS}, retry: {RETRY_SECS}, retries: {RETRIES} }};\n{SCRIPT}",
        js_string(web_bridge::PAGE_URL),
        web_bridge::PORT,
        js_string(web_bridge::PROTOCOL)
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

/// One message for the bridge page: the chunks to show now, and the chunks of the whole
/// override set the game shows once the script applies them, which the page keeps so the
/// next game start can restore it before DeadTune is back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    pub seq: u32,
    pub titles: Vec<String>,
    pub keep: Vec<String>,
}

#[derive(Clone, Debug)]
struct Flight {
    seq: u32,
    /// The override set the script shows once it applies this message.
    target: Vec<LiveRule>,
    at: Instant,
}

/// One message in flight at a time. It keeps the override set the game should show
/// (`desired`) and the set the script last acknowledged (`applied`), so it decides itself
/// between a patch and a full message, and a message left unacknowledged for `TIMEOUT`
/// goes again as a full one. Pure: the caller hands the posts to the bridge and feeds back
/// the script's acks.
#[derive(Clone, Debug)]
pub struct Mailbox {
    base: String,
    next_seq: u32,
    desired: Vec<LiveRule>,
    applied: Option<Vec<LiveRule>>,
    flight: Option<Flight>,
}

impl Mailbox {
    /// `first_seq` should differ between DeadTune runs (seed it from the clock): the script
    /// ignores a message whose seq it applied last. Seq 0 is never used; a new page asks
    /// for anything newer than 0.
    pub fn new(base: &str, first_seq: u32) -> Mailbox {
        Mailbox {
            base: base.to_string(),
            next_seq: first_seq.max(1),
            desired: Vec::new(),
            // The script may hold an earlier run's overrides, restored or live, so the
            // first message replaces whatever it has.
            applied: None,
            flight: None,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// The override set the game should show from now on, and the message for it. It
    /// replaces the one in flight: the page only ever hands over the latest.
    pub fn send(&mut self, desired: Vec<LiveRule>, now: Instant) -> Option<Post> {
        self.desired = desired;
        self.flight = None;
        self.poll(now)
    }

    /// The script loaded again: it shows the baked HUD, or what the page restored.
    pub fn reload(&mut self) {
        self.applied = None;
        self.flight = None;
    }

    /// The next message to post, if any: the change since the last acknowledged one, or
    /// everything again once a message went unanswered for `TIMEOUT`.
    pub fn poll(&mut self, now: Instant) -> Option<Post> {
        if let Some(flight) = &self.flight {
            if now.saturating_duration_since(flight.at) < TIMEOUT {
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
        self.next_seq = seq.wrapping_add(1).max(1);
        self.flight = Some(Flight {
            seq,
            target: self.desired.clone(),
            at: now,
        });
        let message = |kind, rules| {
            titles(&Message {
                seq,
                base: self.base.clone(),
                kind,
                rules,
            })
        };
        Some(Post {
            seq,
            keep: message(Kind::Full, self.desired.clone()),
            titles: message(kind, rules),
        })
    }

    /// The script applied message `seq`.
    pub fn ack(&mut self, seq: u32) {
        if let Some(flight) = self.flight.take_if(|f| f.seq == seq) {
            self.applied = Some(flight.target);
        }
    }

    /// A message is in flight or waiting to go.
    pub fn pending(&self) -> bool {
        self.flight.is_some() || self.applied.as_ref() != Some(&self.desired)
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
        for c in [' ', '\r', '\n', 'ü', '\t'] {
            assert!(!text.contains(c), "{c:?} in {text}");
        }
        assert_eq!(parse_titles(&titles(&m)).unwrap(), m);
        let empty = msg(8, Kind::Full, Vec::new());
        let t = titles(&empty);
        assert_eq!(t, ["dt1 8 1/1 0badf00d full "]);
        assert_eq!(
            parse_titles(&[t[0].trim_end().to_string()]).unwrap(),
            empty,
            "the browser trims a title's trailing space"
        );
    }

    #[test]
    fn one_element_edit_is_one_title() {
        let drag = rules(&moved(-560)).unwrap();
        assert_eq!(drag.len(), 1, "{drag:?}");
        let t = titles(&msg(1, Kind::Patch, drag.clone()));
        assert_eq!(t.len(), 1, "{t:?}");
        assert_eq!(parse_titles(&t).unwrap().rules, drag);
    }

    #[test]
    fn long_messages_split_into_titles_that_reassemble_in_any_order() {
        let m = msg(41, Kind::Full, many(200));
        let mut t = titles(&m);
        assert!(t.len() > 3, "{}", t.len());
        for (i, title) in t.iter().enumerate() {
            assert!(
                title.starts_with(&format!("dt1 41 {}/{} 0badf00d full ", i + 1, t.len())),
                "{title}"
            );
            assert!(title.len() <= CHUNK_CHARS + 40, "{title}");
            assert!(!title.contains("  "), "a title collapses double spaces");
        }
        t.reverse();
        assert_eq!(parse_titles(&t).unwrap(), m);
        assert_eq!(parse_titles(&t[..2]), Err(CodecError::Missing(1, t.len())));
        assert_eq!(parse_titles(&[]), Err(CodecError::Empty));
        assert!(matches!(
            parse_titles(&["DTLIVE ready".to_string()]),
            Err(CodecError::Chunk(_))
        ));
        let other = titles(&msg(42, Kind::Full, many(30)));
        assert!(
            matches!(
                parse_titles(&[t[0].clone(), other[1].clone()]),
                Err(CodecError::Chunk(_))
            ),
            "chunks of two messages don't mix"
        );
        assert!(matches!(
            parse_titles(&["dt1 41 3/2 0badf00d full x".to_string()]),
            Err(CodecError::Chunk(_))
        ));
    }

    #[test]
    fn mailbox_keeps_one_message_in_flight_until_the_ack() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut mb = Mailbox::new("0badf00d", 5);
        assert!(mb.pending(), "a new session owes the script a full message");
        let a = rule("#TopBar", "opacity", "0.5");
        let first = mb.send(vec![a.clone()], t0).unwrap();
        assert_eq!(first.seq, 5);
        assert_eq!(parse_titles(&first.titles).unwrap().kind, Kind::Full);
        assert_eq!(
            first.keep, first.titles,
            "a full message is what the page keeps"
        );
        assert_eq!(
            mb.poll(ms(500)),
            None,
            "nothing more until the script answers"
        );
        mb.ack(4);
        assert!(mb.pending(), "an old seq is no answer");
        mb.ack(5);
        assert!(!mb.pending());
        assert_eq!(mb.poll(ms(600)), None, "nothing changed");
        let b = rule("#Chat", "opacity", "0.2");
        let second = mb.send(vec![a.clone(), b.clone()], ms(700)).unwrap();
        assert_eq!(second.seq, 6);
        let patch = parse_titles(&second.titles).unwrap();
        assert_eq!((patch.kind, patch.rules), (Kind::Patch, vec![b.clone()]));
        let keep = parse_titles(&second.keep).unwrap();
        assert_eq!(
            (keep.seq, keep.kind, keep.rules),
            (6, Kind::Full, vec![a, b]),
            "the page keeps the whole set under the same seq"
        );
    }

    #[test]
    fn mailbox_replaces_the_message_in_flight_and_resends_after_the_timeout() {
        let t0 = Instant::now();
        let mut mb = Mailbox::new("0badf00d", 5);
        mb.send(vec![rule("#TopBar", "opacity", "0.5")], t0)
            .unwrap();
        let newer = mb
            .send(
                vec![rule("#TopBar", "opacity", "0.6")],
                t0 + Duration::from_millis(10),
            )
            .unwrap();
        assert_eq!(newer.seq, 6);
        assert_eq!(parse_titles(&newer.titles).unwrap().kind, Kind::Full);
        mb.ack(5);
        assert!(mb.pending(), "the replaced message's ack doesn't count");
        assert_eq!(mb.poll(t0 + TIMEOUT - Duration::from_millis(1)), None);
        let again = mb.poll(t0 + TIMEOUT + Duration::from_millis(10)).unwrap();
        assert_eq!(again.seq, 7);
        assert_eq!(parse_titles(&again.titles).unwrap().kind, Kind::Full);
    }

    #[test]
    fn mailbox_reload_starts_from_a_full_message() {
        let t0 = Instant::now();
        let mut mb = Mailbox::new("0badf00d", 1);
        let a = rule("#TopBar", "opacity", "0.5");
        let first = mb.send(vec![a.clone()], t0).unwrap();
        mb.ack(first.seq);
        assert_eq!(mb.poll(t0), None);
        mb.reload();
        let again = mb.poll(t0).unwrap();
        let m = parse_titles(&again.titles).unwrap();
        assert_eq!((m.kind, m.rules), (Kind::Full, vec![a]));
    }

    #[test]
    fn mailbox_never_uses_seq_0() {
        let t0 = Instant::now();
        let mut mb = Mailbox::new("0badf00d", 0);
        assert_eq!(mb.send(Vec::new(), t0).unwrap().seq, 1);
        let mut mb = Mailbox::new("0badf00d", u32::MAX);
        assert_eq!(mb.send(Vec::new(), t0).unwrap().seq, u32::MAX);
        assert_eq!(mb.send(Vec::new(), t0).unwrap().seq, 1);
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
                "DEADTUNE_LIVE web fetch blocked TypeError: Failed to fetch",
                LiveLine::Web("fetch blocked TypeError: Failed to fetch".into()),
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
            "DEADTUNE_LIVE web",
            "[InputService] exec: couldn't exec '{}cfg/deadtune_hud.cfg', unable to read file",
        ] {
            assert_eq!(parse_line(line), None, "{line}");
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
            let m = msg(1, Kind::Full, got.clone());
            let back = parse_titles(&titles(&m)).unwrap();
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
    fn script_carries_the_base_and_compile_adds_it() {
        let text = script("0badf00d");
        assert!(
            text.starts_with(
                "var DT_LIVE = { base: \"0badf00d\", page: \"https://simulieren.github.io/deadtune/bridge/\", port: 47613, protocol: \"DTLIVE:v2\", beat: 1, sleepBeat: 3, need: 0.5, quiet: 30, retry: 20, retries: 5 };\n(function () {"
            ),
            "{text}"
        );
        assert!(
            !text.contains("CitadelConCommand"),
            "the script runs no console command"
        );
        assert_eq!(js_string("a\"b\\c\u{e9}"), "\"a\\\"b\\\\c\\u00e9\"");
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
        assert!(edit.panels.is_empty(), "the script makes its own web panel");
        assert_eq!(patch.own_files[OWN_SCRIPT], script(&base_id(&l).unwrap()));
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
        assert!(!text.contains("DtLive"), "no hidden panels in the layout");
        let resource = inject::script_resource(&patch.own_files[OWN_SCRIPT]);
        let parsed = crate::hud::resource::Resource::parse(&resource).unwrap();
        assert!(
            parsed.blocks[0]
                .data
                .starts_with(b"var DT_LIVE = { base: \"")
        );
    }
}
