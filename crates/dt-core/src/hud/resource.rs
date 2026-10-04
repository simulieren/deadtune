//! Source 2 compiled resources (`.vcss_c` and friends), header version 12.
//! We never compile; we clone a real file and swap the text in its DATA block,
//! keeping RED2 and SrMa. Format notes: research/hud/vpk-and-compiled-resources.md section 2.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub name: [u8; 4],
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub header_version: u16,
    pub type_version: u16,
    pub blocks: Vec<Block>,
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("truncated or malformed resource: {0}")]
    Malformed(String),
    #[error("unsupported header version {0}")]
    Version(u16),
    #[error("missing {0} block")]
    MissingBlock(&'static str),
    #[error("DATA text is not UTF-8")]
    NotUtf8,
}

impl Resource {
    pub fn parse(bytes: &[u8]) -> Result<Resource, ResourceError> {
        todo!()
    }

    /// Re-serializes with 16-byte aligned blocks; `parse(x).to_bytes() == x` for
    /// compiler output.
    pub fn to_bytes(&self) -> Vec<u8> {
        todo!()
    }

    pub fn block(&self, name: &[u8; 4]) -> Option<&Block> {
        self.blocks.iter().find(|b| &b.name == name)
    }
}

/// The CSS text of a compiled stylesheet (`DATA[6..]`).
pub fn style_text(res: &Resource) -> Result<&str, ResourceError> {
    todo!()
}

/// Replaces the stylesheet text and recomputes the DATA prefix as
/// `source_crc ^ crc32(text)`, where `source_crc = old_prefix ^ crc32(old_text)`.
pub fn with_style_text(res: &Resource, text: &str) -> Result<Resource, ResourceError> {
    todo!()
}

/// `compiled` with `css` (already minified) appended to its stylesheet text.
pub fn append_style(compiled: &[u8], css: &str) -> Result<Vec<u8>, ResourceError> {
    todo!()
}
