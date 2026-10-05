//! Adds DeadTune's own scripts, stylesheets and panels to one of the game's Panorama
//! layouts without replacing it: the installed game's compiled `.vxml_c` is decoded
//! (`kv3`), the additions go in at named anchors, and the result is written back as a
//! plaintext layout resource, the form a shipped minimap mod already uses in game.
//! Our own script and style files are plaintext resources under `SCRIPTS_DIR` and
//! `STYLES_DIR`, so nothing of the game's is ever overwritten by them.
//! Format notes and the in-game evidence: research/hud/top-bar/NOTES.md sections 2 and 3.

use super::crc32::crc32;
use super::kv3::{self, Value};
use super::resource::{self, Block, Resource};

/// Our scripts live here; `s2r://` includes point at them.
pub const SCRIPTS_DIR: &str = "panorama/scripts/deadtune/";
/// Our stylesheets live here.
pub const STYLES_DIR: &str = "panorama/styles/deadtune/";

const HEADER_VERSION: u16 = 12;
const LAYOUT_TYPE_VERSION: u16 = 3;
const STYLE_TYPE_VERSION: u16 = 3;
/// Scripts of this version and up keep their text bare in DATA.
pub const SCRIPT_TYPE_VERSION: u16 = 4;

/// One node of a layout: a panel, or one of the structural tags (`root`, `styles`,
/// `scripts`, `snippets`, `snippet`, `include`, `script`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// A `<script>` body.
    pub text: Option<String>,
}

impl Element {
    pub fn new(tag: &str) -> Element {
        Element {
            tag: tag.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
            text: None,
        }
    }

    pub fn attr(mut self, name: &str, value: &str) -> Element {
        self.attrs.push((name.to_string(), value.to_string()));
        self
    }

    pub fn child(mut self, child: Element) -> Element {
        self.children.push(child);
        self
    }

    pub fn include(src: &str) -> Element {
        Element::new("include").attr("src", src)
    }

    pub fn id(&self) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v.as_str())
    }

    /// The parent of the element with this id and its index among the children.
    fn locate(&mut self, id: &str) -> Option<(&mut Element, usize)> {
        if let Some(i) = self.children.iter().position(|c| c.id() == Some(id)) {
            return Some((self, i));
        }
        self.children.iter_mut().find_map(|c| c.locate(id))
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut Element> {
        let (parent, i) = self.locate(id)?;
        Some(&mut parent.children[i])
    }

    fn section_mut(&mut self, tag: &str, insert_at: usize) -> &mut Element {
        let at = match self.children.iter().position(|c| c.tag == tag) {
            Some(i) => i,
            None => {
                let at = insert_at.min(self.children.len());
                self.children.insert(at, Element::new(tag));
                at
            }
        };
        &mut self.children[at]
    }
}

/// Where an added panel goes, by the `id` attribute of a panel the game's layout has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    AppendTo(String),
    Before(String),
    After(String),
}

/// One layout's additions. Includes are `s2r://` paths. Applying the same edit twice adds
/// nothing the second time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutEdit {
    pub style_includes: Vec<String>,
    pub script_includes: Vec<String>,
    pub panels: Vec<(Anchor, Element)>,
}

impl LayoutEdit {
    pub fn is_empty(&self) -> bool {
        self.style_includes.is_empty() && self.script_includes.is_empty() && self.panels.is_empty()
    }
}

/// A texture the layout references, for the plaintext resource's image table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageEntry {
    pub name: String,
    pub width: u16,
    pub height: u16,
    pub crc: u32,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InjectError {
    #[error(transparent)]
    Resource(#[from] ResourceErrorText),
    #[error("layout AST: {0}")]
    Kv3(#[from] kv3::Kv3Error),
    #[error("layout AST has no m_AST.m_pRoot")]
    NoRoot,
    #[error("layout AST node type {0:?} is unknown")]
    NodeType(String),
    #[error("layout has no compiled AST to rebuild from")]
    NotCompiled,
    #[error("no panel with id {0:?} to anchor on")]
    AnchorMissing(String),
}

/// `ResourceError` carries no `PartialEq`; its message does.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct ResourceErrorText(String);

impl From<resource::ResourceError> for InjectError {
    fn from(e: resource::ResourceError) -> Self {
        InjectError::Resource(ResourceErrorText(e.to_string()))
    }
}

fn etype(node: &Value) -> &str {
    node.get("eType").and_then(Value::as_str).unwrap_or("")
}

fn name(node: &Value) -> String {
    node.get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn reference(node: &Value) -> Result<String, InjectError> {
    let value = name(node);
    Ok(match etype(node) {
        "REFERENCE_COMPILED" => format!("s2r://{value}"),
        "REFERENCE_PASSTHROUGH" => format!("file://{value}"),
        "PANEL_ATTRIBUTE_VALUE" => value,
        other => return Err(InjectError::NodeType(other.to_string())),
    })
}

fn sub_nodes(node: &Value) -> Vec<&Value> {
    if let Some(items) = node.get("vecChildren").and_then(Value::as_array) {
        items.iter().collect()
    } else if let Some(child) = node.get("child") {
        vec![child]
    } else {
        Vec::new()
    }
}

fn element(node: &Value) -> Result<Element, InjectError> {
    let kind = etype(node);
    let tag = match kind {
        "ROOT" => "root",
        "STYLES" => "styles",
        "SCRIPTS" => "scripts",
        "SNIPPETS" => "snippets",
        "SNIPPET" => "snippet",
        "INCLUDE" => {
            let child = node.get("child").ok_or(InjectError::NoRoot)?;
            return Ok(Element::include(&reference(child)?));
        }
        "SCRIPT_BODY" => {
            let mut e = Element::new("script");
            e.text = Some(name(node));
            return Ok(e);
        }
        "PANEL" => "",
        other => return Err(InjectError::NodeType(other.to_string())),
    };
    let panel_name = name(node);
    let mut e = Element::new(if kind == "PANEL" { &panel_name } else { tag });
    if kind == "SNIPPET" {
        e.attrs.push(("name".to_string(), name(node)));
    }
    for sub in sub_nodes(node) {
        if etype(sub) == "PANEL_ATTRIBUTE" {
            let value = sub.get("child").ok_or(InjectError::NoRoot)?;
            e.attrs.push((name(sub), reference(value)?));
        } else {
            e.children.push(element(sub)?);
        }
    }
    Ok(e)
}

/// The layout tree of a decoded `LaCo` block.
pub fn from_kv3(doc: &Value) -> Result<Element, InjectError> {
    let root = doc
        .get("m_AST")
        .and_then(|ast| ast.get("m_pRoot"))
        .ok_or(InjectError::NoRoot)?;
    element(root)
}

/// The game's compiled layout as a tree. A layout stored as text (older or third-party
/// files) has no AST to read; that is `NotCompiled`.
pub fn tree(compiled: &[u8]) -> Result<Element, InjectError> {
    let res = Resource::parse(compiled)?;
    let laco = res.block(b"LaCo").ok_or(InjectError::NotCompiled)?;
    from_kv3(&kv3::parse(&laco.data)?.root)
}

/// The XML text of a compiled or plaintext layout resource.
pub fn layout_text(compiled: &[u8]) -> Result<String, InjectError> {
    let res = Resource::parse(compiled)?;
    match res.block(b"LaCo") {
        Some(laco) => Ok(to_xml(&from_kv3(&kv3::parse(&laco.data)?.root)?, "")),
        None => Ok(resource::style_text(&res)?.to_string()),
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn write(e: &Element, depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push('\t');
    }
    if e.tag == "script" {
        out.push_str("<script><![CDATA[");
        out.push_str(e.text.as_deref().unwrap_or_default());
        out.push_str("]]></script>\n");
        return;
    }
    out.push('<');
    out.push_str(&e.tag);
    for (k, v) in &e.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        out.push_str(&escape(v));
        out.push('"');
    }
    if e.children.is_empty() {
        out.push_str(" />\n");
        return;
    }
    out.push_str(">\n");
    for c in &e.children {
        write(c, depth + 1, out);
    }
    for _ in 0..depth {
        out.push('\t');
    }
    out.push_str("</");
    out.push_str(&e.tag);
    out.push_str(">\n");
}

/// Layout XML in the form Source 2 Viewer prints and the engine reads back: tabs, one
/// node per line, `&apos;`-style escapes. `note` becomes a leading comment when not empty.
pub fn to_xml(root: &Element, note: &str) -> String {
    let mut out = String::new();
    if !note.is_empty() {
        out.push_str("<!-- ");
        out.push_str(&note.replace("--", "- -"));
        out.push_str(" -->\n");
    }
    write(root, 0, &mut out);
    out
}

/// Adds the edit to the tree. Includes already present are left alone; a panel whose
/// `id` is already present at its anchor is not added again.
pub fn apply(root: &mut Element, edit: &LayoutEdit) -> Result<(), InjectError> {
    if !edit.style_includes.is_empty() {
        let styles = root.section_mut("styles", 0);
        add_includes(styles, &edit.style_includes);
    }
    if !edit.script_includes.is_empty() {
        let after_styles = root
            .children
            .iter()
            .position(|c| c.tag == "styles")
            .map_or(0, |i| i + 1);
        let scripts = root.section_mut("scripts", after_styles);
        add_includes(scripts, &edit.script_includes);
    }
    for (anchor, panel) in &edit.panels {
        let id = match anchor {
            Anchor::AppendTo(id) | Anchor::Before(id) | Anchor::After(id) => id,
        };
        let present = |e: &Element| {
            panel
                .id()
                .is_some_and(|p| e.children.iter().any(|c| c.id() == Some(p)))
        };
        match anchor {
            Anchor::AppendTo(_) => {
                let target = root
                    .find_mut(id)
                    .ok_or_else(|| InjectError::AnchorMissing(id.clone()))?;
                if !present(target) {
                    target.children.push(panel.clone());
                }
            }
            Anchor::Before(_) | Anchor::After(_) => {
                let (parent, i) = root
                    .locate(id)
                    .ok_or_else(|| InjectError::AnchorMissing(id.clone()))?;
                if !present(parent) {
                    let at = if matches!(anchor, Anchor::After(_)) {
                        i + 1
                    } else {
                        i
                    };
                    parent.children.insert(at, panel.clone());
                }
            }
        }
    }
    Ok(())
}

fn add_includes(section: &mut Element, srcs: &[String]) {
    for src in srcs {
        let have = section
            .children
            .iter()
            .any(|c| c.tag == "include" && c.attrs.iter().any(|(k, v)| k == "src" && v == src));
        if !have {
            section.children.push(Element::include(src));
        }
    }
}

fn plaintext(type_version: u16, data: Vec<u8>) -> Vec<u8> {
    Resource {
        header_version: HEADER_VERSION,
        type_version,
        blocks: vec![Block {
            name: *b"DATA",
            data,
        }],
    }
    .to_bytes()
}

fn prefixed(text: &str, images: &[ImageEntry]) -> Vec<u8> {
    let mut data = Vec::with_capacity(text.len() + 6);
    data.extend_from_slice(&crc32(text.as_bytes()).to_le_bytes());
    data.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for image in images {
        data.extend_from_slice(image.name.as_bytes());
        data.push(0);
        data.extend_from_slice(&image.width.to_le_bytes());
        data.extend_from_slice(&image.height.to_le_bytes());
        data.extend_from_slice(&image.crc.to_le_bytes());
    }
    data.extend_from_slice(text.as_bytes());
    data
}

/// A `.vxml_c` holding `xml` as text.
pub fn layout_resource(xml: &str, images: &[ImageEntry]) -> Vec<u8> {
    plaintext(LAYOUT_TYPE_VERSION, prefixed(xml, images))
}

/// A `.vcss_c` holding `css` as text, no image table.
pub fn style_resource(css: &str) -> Vec<u8> {
    plaintext(STYLE_TYPE_VERSION, prefixed(css, &[]))
}

/// A `.vjs_c` holding `js` as text.
pub fn script_resource(js: &str) -> Vec<u8> {
    plaintext(SCRIPT_TYPE_VERSION, js.as_bytes().to_vec())
}

/// The game's compiled layout with `edit` applied, as a plaintext `.vxml_c`.
pub fn patched_layout(
    compiled: &[u8],
    edit: &LayoutEdit,
    images: &[ImageEntry],
    note: &str,
) -> Result<Vec<u8>, InjectError> {
    let mut root = tree(compiled)?;
    apply(&mut root, edit)?;
    Ok(layout_resource(&to_xml(&root, note), images))
}

/// True when every line of `original` appears in `ours` in order: ours only adds lines.
pub fn extends(ours: &str, original: &str) -> bool {
    let mut lines = ours.lines();
    original
        .lines()
        .all(|want| lines.by_ref().any(|line| line == want))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOP_BAR: &[u8] = include_bytes!("../../tests/fixtures/hud/top_bar_vanilla.vxml_c");
    const MINIMAP: &[u8] = include_bytes!("../../tests/fixtures/hud/hud_minimap_vanilla.vxml_c");
    const HALLOWEEN: &[u8] =
        include_bytes!("../../tests/fixtures/hud/vrf_dashboard_halloween2015.vxml_c");

    fn top_bar_edit() -> LayoutEdit {
        LayoutEdit {
            style_includes: vec!["s2r://panorama/styles/deadtune/top_bar.vcss_c".into()],
            script_includes: vec!["s2r://panorama/scripts/deadtune/top_bar.vjs_c".into()],
            panels: vec![(
                Anchor::AppendTo("RejuvenatorCharges".into()),
                Element::new("Panel")
                    .attr("id", "DtSpawnTimers")
                    .attr("hittest", "false")
                    .child(
                        Element::new("Label")
                            .attr("id", "DtPowerupTimer")
                            .attr("text", "5:00"),
                    ),
            )],
        }
    }

    #[test]
    fn prints_the_vanilla_top_bar_like_source2viewer() {
        let xml = to_xml(&tree(TOP_BAR).unwrap(), "");
        let lines: Vec<&str> = xml.lines().collect();
        assert_eq!(lines[0], "<root>");
        assert_eq!(lines[1], "\t<styles>");
        assert_eq!(
            lines[2],
            "\t\t<include src=\"s2r://panorama/styles/citadel_base_styles.vcss\" />"
        );
        assert_eq!(lines[7], "\t<CitadelHudTopBar hittest=\"false\">");
        assert!(lines.contains(
            &"\t\t\t<Label class=\"GameTime\" id=\"GameTime\" text=\"{s:game_clock}\" />"
        ));
        assert!(lines.contains(&"\t\t<Panel id=\"RejuvenatorCharges\">"));
        assert_eq!(lines.last(), Some(&"</root>"));
        assert_eq!(layout_text(TOP_BAR).unwrap(), xml);
        assert_eq!(lines.iter().filter(|l| l.contains("<include ")).count(), 4);
    }

    #[test]
    fn snippets_and_references_print() {
        let xml = layout_text(MINIMAP).unwrap();
        assert!(
            xml.contains("\t<snippets>\n\t\t<snippet name=\"MinimapPlayerSnippet\">"),
            "{xml}"
        );
        assert!(xml.contains("src=\"s2r://panorama/images/minimap/player_cone_psd.vtex\""));
        assert!(xml.contains("\t\t</snippet>\n"));
    }

    #[test]
    fn escapes_attribute_values() {
        let e = Element::new("Panel").attr("onactivate", "Do('x') && <b>");
        assert_eq!(
            to_xml(&e, ""),
            "<Panel onactivate=\"Do(&apos;x&apos;) &amp;&amp; &lt;b&gt;\" />\n"
        );
        let mut script = Element::new("script");
        script.text = Some("$.Msg('hi')".into());
        assert_eq!(
            to_xml(&Element::new("scripts").child(script), "x -- y"),
            "<!-- x - - y -->\n<scripts>\n\t<script><![CDATA[$.Msg('hi')]]></script>\n</scripts>\n"
        );
    }

    #[test]
    fn applies_includes_and_panels_once() {
        let mut root = tree(TOP_BAR).unwrap();
        apply(&mut root, &top_bar_edit()).unwrap();
        apply(&mut root, &top_bar_edit()).unwrap();
        let xml = to_xml(&root, "");
        assert_eq!(xml.matches("deadtune/top_bar.vcss_c").count(), 1);
        assert_eq!(xml.matches("deadtune/top_bar.vjs_c").count(), 1);
        assert_eq!(xml.matches("DtSpawnTimers").count(), 1);
        let styles_end = xml.find("\t</styles>\n").unwrap();
        assert_eq!(
            &xml[styles_end..styles_end + 100],
            "\t</styles>\n\t<scripts>\n\t\t<include src=\"s2r://panorama/scripts/deadtune/top_bar.vjs_c\" />\n\t</scripts>\n"
        );
        let charges = xml.find("<Panel id=\"RejuvenatorCharges\">").unwrap();
        let end = xml[charges..].find("\n\t\t</Panel>").unwrap() + charges;
        assert!(
            xml[charges..end].ends_with(
                "\t\t\t<Panel id=\"DtSpawnTimers\" hittest=\"false\">\n\t\t\t\t<Label id=\"DtPowerupTimer\" text=\"5:00\" />\n\t\t\t</Panel>"
            ),
            "{}",
            &xml[charges..end]
        );
        assert!(extends(&xml, &layout_text(TOP_BAR).unwrap()));
        assert!(!extends(&layout_text(TOP_BAR).unwrap(), &xml));
    }

    #[test]
    fn before_and_after_anchors_and_missing_anchor() {
        let mut root = tree(TOP_BAR).unwrap();
        let edit = LayoutEdit {
            panels: vec![
                (
                    Anchor::Before("TeamsContainer".into()),
                    Element::new("Panel").attr("id", "DtBefore"),
                ),
                (
                    Anchor::After("TeamsContainer".into()),
                    Element::new("Panel").attr("id", "DtAfter"),
                ),
            ],
            ..LayoutEdit::default()
        };
        apply(&mut root, &edit).unwrap();
        let xml = to_xml(&root, "");
        let (b, t, a) = (
            xml.find("DtBefore").unwrap(),
            xml.find("id=\"TeamsContainer\"").unwrap(),
            xml.find("DtAfter").unwrap(),
        );
        assert!(b < t && t < a);
        let missing = LayoutEdit {
            panels: vec![(Anchor::AppendTo("Nope".into()), Element::new("Panel"))],
            ..LayoutEdit::default()
        };
        assert_eq!(
            apply(&mut root, &missing),
            Err(InjectError::AnchorMissing("Nope".into()))
        );
    }

    #[test]
    fn creates_sections_when_missing() {
        let mut root = Element::new("root").child(Element::new("CitadelHudTopBar"));
        apply(&mut root, &top_bar_edit().panels_cleared()).unwrap();
        assert_eq!(root.children[0].tag, "styles");
        assert_eq!(root.children[1].tag, "scripts");
        assert_eq!(root.children[2].tag, "CitadelHudTopBar");
    }

    impl LayoutEdit {
        fn panels_cleared(mut self) -> LayoutEdit {
            self.panels.clear();
            self
        }
    }

    #[test]
    fn patched_layout_is_a_plaintext_resource() {
        let images = [ImageEntry {
            name: "panorama/images/hud/spec_icon_png.vtex".into(),
            width: 64,
            height: 32,
            crc: 0xDEADBEEF,
        }];
        let bytes = patched_layout(TOP_BAR, &top_bar_edit(), &images, "DeadTune test").unwrap();
        let res = Resource::parse(&bytes).unwrap();
        assert_eq!(res.header_version, 12);
        assert_eq!(res.type_version, 3);
        assert_eq!(res.blocks.len(), 1);
        assert_eq!(res.to_bytes(), bytes);
        let text = resource::style_text(&res).unwrap();
        assert!(text.starts_with("<!-- DeadTune test -->\n<root>\n"));
        assert!(text.contains("DtSpawnTimers"));
        let data = &res.blocks[0].data;
        assert_eq!(
            u32::from_le_bytes(data[..4].try_into().unwrap()),
            crc32(text.as_bytes())
        );
        assert_eq!(&data[4..6], &[1, 0]);
        let table = resource::image_table(&res).unwrap();
        assert_eq!(table.len(), 2 + images[0].name.len() + 1 + 8);
        assert_eq!(&table[table.len() - 4..], &0xDEADBEEFu32.to_le_bytes());
        assert_eq!(layout_text(&bytes).unwrap(), text);
        assert_eq!(tree(&bytes), Err(InjectError::NotCompiled));
    }

    #[test]
    fn plaintext_valve_layout_reads_back() {
        let text = layout_text(HALLOWEEN).unwrap();
        assert!(text.starts_with("<root>"), "{}", &text[..40]);
        assert!(text.contains("halloween2015"));
        assert_eq!(tree(HALLOWEEN), Err(InjectError::NotCompiled));
    }

    #[test]
    fn script_and_style_resources() {
        let js = script_resource("(function(){ $.Msg('x'); })();");
        let res = Resource::parse(&js).unwrap();
        assert_eq!((res.type_version, res.blocks.len()), (4, 1));
        assert_eq!(res.blocks[0].data, b"(function(){ $.Msg('x'); })();");
        let css = style_resource("#DtPowerupTimer{color:#fff;}");
        let res = Resource::parse(&css).unwrap();
        assert_eq!(res.type_version, 3);
        assert_eq!(
            resource::style_text(&res).unwrap(),
            "#DtPowerupTimer{color:#fff;}"
        );
        assert_eq!(
            resource::source_crc(&res).unwrap(),
            0,
            "prefix is the text's own crc"
        );
        assert_eq!(resource::image_table(&res).unwrap(), &[0, 0]);
    }

    #[test]
    fn edits_compare_and_report_emptiness() {
        assert!(LayoutEdit::default().is_empty());
        assert!(!top_bar_edit().is_empty());
        assert!(extends("a\nb\nc\n", "a\nc\n"));
        assert!(!extends("a\nc\n", "a\nb\nc\n"));
    }
}
