//! Source 2 compiled resources (`.vcss_c` and friends), header version 12.
//! We never compile; we clone a real file and swap the text in its DATA block,
//! keeping RED2 and SrMa. Format notes: research/hud/vpk-and-compiled-resources.md section 2.

use super::crc32::crc32;

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

const HEADER_VERSION: u16 = 12;
const TABLE_START: usize = 16;
const ENTRY_LEN: usize = 12;
const ALIGN: usize = 16;
/// DATA layout (ValveResourceFormat `Panorama.Read`): u32 CRC, u16 image count, then per image a
/// NUL-terminated name, u16 width, u16 height and, from resource version 3, a u32 CRC; then text.
const DATA_IMAGES_OFFSET: usize = 6;

fn malformed(msg: &str) -> ResourceError {
    ResourceError::Malformed(msg.to_string())
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, ResourceError> {
    let s = b
        .get(at..at + 2)
        .ok_or_else(|| malformed("header truncated"))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ResourceError> {
    let s = b
        .get(at..at + 4)
        .ok_or_else(|| malformed("header truncated"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn pad16(n: usize) -> usize {
    (n + ALIGN - 1) & !(ALIGN - 1)
}

impl Resource {
    pub fn parse(bytes: &[u8]) -> Result<Resource, ResourceError> {
        let file_size = u32_at(bytes, 0)? as usize;
        if file_size > bytes.len() {
            return Err(malformed("declared file size exceeds input"));
        }
        let header_version = u16_at(bytes, 4)?;
        if header_version != HEADER_VERSION {
            return Err(ResourceError::Version(header_version));
        }
        let type_version = u16_at(bytes, 6)?;
        let table_off = u32_at(bytes, 8)? as usize;
        let count = u32_at(bytes, 12)? as usize;
        let table = 8usize
            .checked_add(table_off)
            .ok_or_else(|| malformed("block table offset overflow"))?;

        let mut blocks = Vec::new();
        for i in 0..count {
            let entry = i
                .checked_mul(ENTRY_LEN)
                .and_then(|n| n.checked_add(table))
                .ok_or_else(|| malformed("block table overflow"))?;
            let name = bytes
                .get(entry..entry + 4)
                .ok_or_else(|| malformed("block table truncated"))?;
            let rel = u32_at(bytes, entry + 4)? as usize;
            let size = u32_at(bytes, entry + 8)? as usize;
            let start = (entry + 4)
                .checked_add(rel)
                .ok_or_else(|| malformed("block offset overflow"))?;
            let end = start
                .checked_add(size)
                .filter(|&e| e <= file_size)
                .ok_or_else(|| malformed("block extends past end of file"))?;
            blocks.push(Block {
                name: [name[0], name[1], name[2], name[3]],
                data: bytes[start..end].to_vec(),
            });
        }
        Ok(Resource {
            header_version,
            type_version,
            blocks,
        })
    }

    /// Re-serializes with 16-byte aligned blocks; `parse(x).to_bytes() == x` for
    /// compiler output.
    pub fn to_bytes(&self) -> Vec<u8> {
        let n = self.blocks.len();
        let mut offsets = Vec::with_capacity(n);
        let mut pos = pad16(TABLE_START + n * ENTRY_LEN);
        let mut end = pos;
        for b in &self.blocks {
            offsets.push(pos);
            end = pos + b.data.len();
            pos = pad16(end);
        }

        let mut out = vec![0u8; end];
        out[0..4].copy_from_slice(&(end as u32).to_le_bytes());
        out[4..6].copy_from_slice(&self.header_version.to_le_bytes());
        out[6..8].copy_from_slice(&self.type_version.to_le_bytes());
        out[8..12].copy_from_slice(&8u32.to_le_bytes());
        out[12..16].copy_from_slice(&(n as u32).to_le_bytes());
        for (i, (b, &off)) in self.blocks.iter().zip(&offsets).enumerate() {
            let e = TABLE_START + i * ENTRY_LEN;
            out[e..e + 4].copy_from_slice(&b.name);
            out[e + 4..e + 8].copy_from_slice(&((off - (e + 4)) as u32).to_le_bytes());
            out[e + 8..e + 12].copy_from_slice(&(b.data.len() as u32).to_le_bytes());
            out[off..off + b.data.len()].copy_from_slice(&b.data);
        }
        out
    }

    pub fn block(&self, name: &[u8; 4]) -> Option<&Block> {
        self.blocks.iter().find(|b| &b.name == name)
    }
}

fn data_block(res: &Resource) -> Result<&Block, ResourceError> {
    res.block(b"DATA")
        .ok_or(ResourceError::MissingBlock("DATA"))
}

/// Byte offset of the stylesheet text inside DATA, past the image table.
fn text_offset(data: &[u8], type_version: u16) -> Result<usize, ResourceError> {
    let images =
        u16_at(data, 4).map_err(|_| malformed("DATA block shorter than its 6-byte prefix"))?;
    let mut pos = DATA_IMAGES_OFFSET;
    for _ in 0..images {
        let nul = data
            .get(pos..)
            .and_then(|rest| rest.iter().position(|&b| b == 0))
            .ok_or_else(|| malformed("DATA image name not terminated"))?;
        pos += nul + 1 + 4 + if type_version >= 3 { 4 } else { 0 };
    }
    if pos > data.len() {
        return Err(malformed("DATA image table runs past the block"));
    }
    Ok(pos)
}

fn data_text(data: &[u8], type_version: u16) -> Result<&str, ResourceError> {
    let text = &data[text_offset(data, type_version)?..];
    std::str::from_utf8(text).map_err(|_| ResourceError::NotUtf8)
}

/// The CSS text of a compiled stylesheet (DATA after the CRC and image table).
pub fn style_text(res: &Resource) -> Result<&str, ResourceError> {
    data_text(&data_block(res)?.data, res.type_version)
}

/// The image table bytes of a compiled stylesheet (DATA between the CRC and the text).
pub fn image_table(res: &Resource) -> Result<&[u8], ResourceError> {
    let data = &data_block(res)?.data;
    Ok(&data[4..text_offset(data, res.type_version)?])
}

/// The compiler's source CRC, which the DATA prefix hides behind the text's own CRC. Two
/// stylesheets built from the same source share it whatever their text says.
pub fn source_crc(res: &Resource) -> Result<u32, ResourceError> {
    let data = &data_block(res)?.data;
    Ok(u32_at(data, 0)? ^ crc32(data_text(data, res.type_version)?.as_bytes()))
}

/// Replaces the stylesheet text, keeping the image table, and recomputes the DATA prefix as
/// `source_crc ^ crc32(text)`, where `source_crc = old_prefix ^ crc32(old_text)`.
pub fn with_style_text(res: &Resource, text: &str) -> Result<Resource, ResourceError> {
    let old = &data_block(res)?.data;
    let offset = text_offset(old, res.type_version)?;
    let prefix = source_crc(res)? ^ crc32(text.as_bytes());

    let mut data = Vec::with_capacity(offset + text.len());
    data.extend_from_slice(&prefix.to_le_bytes());
    data.extend_from_slice(&old[4..offset]);
    data.extend_from_slice(text.as_bytes());

    let mut out = res.clone();
    for b in out.blocks.iter_mut().filter(|b| &b.name == b"DATA") {
        b.data = data.clone();
    }
    Ok(out)
}

/// `compiled` with `css` (already minified) appended to its stylesheet text.
pub fn append_style(compiled: &[u8], css: &str) -> Result<Vec<u8>, ResourceError> {
    let res = Resource::parse(compiled)?;
    let mut text = style_text(&res)?.to_string();
    text.push_str(css);
    Ok(with_style_text(&res, &text)?.to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VANILLA: &[u8] = include_bytes!("../../tests/fixtures/hud/hud_vanilla.vcss_c");
    const SMALL: &[u8] = include_bytes!("../../tests/fixtures/hud/hud_abilities_small.vcss_c");

    /// The live game's base stylesheet lists images before its text; Sqooky's copy didn't.
    fn with_images(bytes: &[u8]) -> Resource {
        let mut res = Resource::parse(bytes).unwrap();
        let version = res.type_version;
        let data = &mut res
            .blocks
            .iter_mut()
            .find(|b| &b.name == b"DATA")
            .unwrap()
            .data;
        let mut table = vec![];
        table.extend_from_slice(&2u16.to_le_bytes());
        for name in ["file://{images}/a.png", "file://{images}/b\u{e9}.svg"] {
            table.extend_from_slice(name.as_bytes());
            table.push(0);
            table.extend_from_slice(&[0xff, 0x00, 0x40, 0x80]);
            if version >= 3 {
                table.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
            }
        }
        data.splice(4..6, table);
        res
    }

    #[test]
    fn reads_and_rewrites_text_after_an_image_table() {
        let plain = Resource::parse(VANILLA).unwrap();
        let res = with_images(VANILLA);
        assert_eq!(style_text(&res).unwrap(), style_text(&plain).unwrap());
        let out = with_style_text(&res, "#x{}").unwrap();
        assert_eq!(style_text(&out).unwrap(), "#x{}");
        let (old, new) = (
            &res.block(b"DATA").unwrap().data,
            &out.block(b"DATA").unwrap().data,
        );
        let offset = text_offset(old, res.type_version).unwrap();
        assert_eq!(
            old[4..offset],
            new[4..offset],
            "image table kept byte for byte"
        );
    }

    #[test]
    fn round_trips_fixtures() {
        for (bytes, len) in [(VANILLA, 112050), (SMALL, 1581)] {
            assert_eq!(bytes.len(), len);
            let res = Resource::parse(bytes).unwrap();
            assert_eq!(res.to_bytes(), bytes);
        }
    }

    #[test]
    fn block_names() {
        let res = Resource::parse(VANILLA).unwrap();
        let names: Vec<&[u8]> = res.blocks.iter().map(|b| &b.name[..]).collect();
        assert_eq!(names, [&b"RED2"[..], b"DATA", b"SrMa"]);
    }

    #[test]
    fn same_text_reproduces_bytes() {
        for bytes in [VANILLA, SMALL] {
            let res = Resource::parse(bytes).unwrap();
            let text = style_text(&res).unwrap();
            let same = with_style_text(&res, text).unwrap();
            assert_eq!(same.to_bytes(), bytes);
        }
    }

    #[test]
    fn new_text_prefix_relation() {
        let res = Resource::parse(SMALL).unwrap();
        let old = &res.block(b"DATA").unwrap().data;
        let source = u32_at(old, 0).unwrap() ^ crc32(style_text(&res).unwrap().as_bytes());
        assert_eq!(source_crc(&res).unwrap(), source);

        let text = ".a{color:red;}";
        let out = with_style_text(&res, text).unwrap();
        let data = &out.block(b"DATA").unwrap().data;
        assert_eq!(u32_at(data, 0).unwrap(), source ^ crc32(text.as_bytes()));
        assert_eq!(&data[4..6], &[0, 0]);
        assert_eq!(
            source_crc(&out).unwrap(),
            source,
            "source crc survives a rewrite"
        );
        assert_eq!(image_table(&out).unwrap(), image_table(&res).unwrap());
        let table = image_table(&with_images(VANILLA)).unwrap().to_vec();
        assert_eq!(&table[..2], &[2, 0], "two images listed");
        assert!(table.len() >= 2 + 26 + 28, "{}", table.len());

        let back = Resource::parse(&out.to_bytes()).unwrap();
        assert_eq!(style_text(&back).unwrap(), text);
        assert_eq!(back.block(b"RED2"), res.block(b"RED2"));
        assert_eq!(back.block(b"SrMa"), res.block(b"SrMa"));
    }

    #[test]
    fn append_style_on_vanilla() {
        let css = "#minimap_persp{width:300px;height:300px;}";
        let out = append_style(VANILLA, css).unwrap();
        let res = Resource::parse(&out).unwrap();
        assert!(style_text(&res).unwrap().ends_with(css));
        assert_eq!(u32_at(&out, 0).unwrap() as usize, out.len());
    }

    #[test]
    fn source_crc_is_in_red2() {
        for bytes in [VANILLA, SMALL] {
            let res = Resource::parse(bytes).unwrap();
            let data = &res.block(b"DATA").unwrap().data;
            let src = u32_at(data, 0).unwrap() ^ crc32(style_text(&res).unwrap().as_bytes());
            let red2 = &res.block(b"RED2").unwrap().data;
            assert!(red2.windows(4).any(|w| w == src.to_le_bytes()));
        }
    }

    #[test]
    fn errors() {
        assert!(matches!(
            Resource::parse(&[0; 4]),
            Err(ResourceError::Malformed(_))
        ));
        let mut res = Resource::parse(SMALL).unwrap();
        res.blocks[1].data.truncate(3);
        assert!(matches!(style_text(&res), Err(ResourceError::Malformed(_))));
        res.blocks[1].data = vec![0, 0, 0, 0, 0, 0, 0xFF];
        assert!(matches!(style_text(&res), Err(ResourceError::NotUtf8)));
        res.blocks.remove(1);
        assert!(matches!(
            style_text(&res),
            Err(ResourceError::MissingBlock("DATA"))
        ));
        let mut bad = SMALL.to_vec();
        bad[4] = 11;
        assert!(matches!(
            Resource::parse(&bad),
            Err(ResourceError::Version(11))
        ));
    }
}
