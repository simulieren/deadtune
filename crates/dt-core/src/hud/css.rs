//! Just enough Panorama CSS for our needs: read the declarations the game ships for
//! a selector (vanilla values for the editor), and emit minified rules. Compiled
//! stylesheets are already minified; at-rules with blocks (`@keyframes`) are skipped.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Selector list exactly as written, e.g. `.gDetailView #minimap_persp,.gScoreboardOpen #minimap_persp`.
    pub selectors: String,
    pub decls: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CssError {
    #[error("unbalanced braces at byte {0}")]
    Unbalanced(usize),
    #[error("comment not closed")]
    OpenComment,
}

/// Top-level style rules in source order. `@define`/`@import` statements and
/// block at-rules are not returned.
pub fn parse_rules(text: &str) -> Result<Vec<Rule>, CssError> {
    todo!()
}

/// Declarations of every rule whose selector list contains exactly `selector`
/// (e.g. `#minimap_persp`), later rules overriding earlier ones per property.
pub fn decls_for(text: &str, selector: &str) -> Result<Vec<(String, String)>, CssError> {
    todo!()
}

/// `sel{a:b;c:d;}` with no whitespace beyond what values need.
pub fn emit_rule(selector: &str, decls: &[(&str, String)]) -> String {
    todo!()
}

/// Strips comments and collapses whitespace the way the Valve compiler does.
pub fn minify(text: &str) -> Result<String, CssError> {
    todo!()
}
