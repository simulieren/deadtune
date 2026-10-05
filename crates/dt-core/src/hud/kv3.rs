//! Binary KeyValues3 reader, versions 4 and 5, LZ4 or uncompressed, plus the legacy
//! `VKV\x03` format (version 1, inline values; third-party compilers still emit it): enough
//! to decode the `LaCo` block of a compiled Panorama layout (`.vxml_c`). Read-only on
//! purpose; DeadTune rebuilds layouts as text (`inject`), so no writer is needed. Format
//! notes: research/hud/top-bar/NOTES.md section 2; reference reader: ValveResourceFormat
//! `BinaryKV3`.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    /// Members in file order; a repeated name keeps its last value.
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(members) => members.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub version: u8,
    pub root: Value,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Kv3Error {
    #[error("not binary KV3 (magic {0:08x})")]
    Magic(u32),
    #[error("binary KV3 version {0} is not supported (4 and 5 are)")]
    Version(u8),
    #[error("compression method {0} is not supported (0 and LZ4 are)")]
    Compression(u32),
    #[error("legacy KV3 encoding {0} is not supported (uncompressed and LZ4 are)")]
    Encoding(String),
    #[error("binary blobs are not supported")]
    Blobs,
    #[error("truncated: {0}")]
    Truncated(&'static str),
    #[error("LZ4 block is malformed")]
    Lz4,
    #[error("unknown node type {0}")]
    NodeType(u8),
    #[error("bad trailer {0:08x}")]
    Trailer(u32),
}

const MAGIC: u32 = 0x4B56_3300;
/// `VKV\x03`: the first binary format, a string table followed by inline nodes.
const LEGACY_MAGIC: u32 = 0x0356_4B56;
const TRAILER: u32 = 0xFFEE_DD00;
const LZ4: u32 = 1;
/// Encoding GUIDs of the legacy header (ValveResourceFormat `KV3_ENCODING_BINARY_*`).
const LEGACY_UNCOMPRESSED: [u8; 16] = [
    0x00, 0x05, 0x86, 0x1B, 0xD8, 0xF7, 0xC1, 0x40, 0xAD, 0x82, 0x75, 0xA4, 0x82, 0x67, 0xE7, 0x14,
];
const LEGACY_LZ4: [u8; 16] = [
    0x8A, 0x34, 0x47, 0x68, 0xA1, 0x63, 0x5C, 0x4F, 0xA1, 0x97, 0x53, 0x80, 0x6F, 0xD9, 0xB1, 0x19,
];

/// Decodes one raw LZ4 block (no frame header) into exactly `out_len` bytes.
pub fn lz4_decode(src: &[u8], out_len: usize) -> Result<Vec<u8>, Kv3Error> {
    let mut out = Vec::with_capacity(out_len);
    let mut i = 0;
    let byte = |i: &mut usize| -> Result<u8, Kv3Error> {
        let b = *src.get(*i).ok_or(Kv3Error::Lz4)?;
        *i += 1;
        Ok(b)
    };
    let extended = |i: &mut usize, mut n: usize| -> Result<usize, Kv3Error> {
        if n == 15 {
            loop {
                let b = byte(i)?;
                n += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        Ok(n)
    };
    while i < src.len() {
        let token = byte(&mut i)?;
        let literals = extended(&mut i, (token >> 4) as usize)?;
        let end = i
            .checked_add(literals)
            .filter(|&e| e <= src.len())
            .ok_or(Kv3Error::Lz4)?;
        out.extend_from_slice(&src[i..end]);
        i = end;
        if i >= src.len() {
            break;
        }
        let offset = u16::from_le_bytes([byte(&mut i)?, byte(&mut i)?]) as usize;
        let length = extended(&mut i, (token & 0xF) as usize)? + 4;
        if offset == 0 || offset > out.len() {
            return Err(Kv3Error::Lz4);
        }
        let start = out.len() - offset;
        for k in 0..length {
            out.push(out[start + k]);
        }
    }
    if out.len() != out_len {
        return Err(Kv3Error::Lz4);
    }
    Ok(out)
}

struct Lane<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Lane<'a> {
    fn new(data: &'a [u8]) -> Lane<'a> {
        Lane { data, pos: 0 }
    }

    fn take<const N: usize>(&mut self, what: &'static str) -> Result<[u8; N], Kv3Error> {
        let bytes = self
            .data
            .get(self.pos..self.pos + N)
            .ok_or(Kv3Error::Truncated(what))?;
        self.pos += N;
        Ok(bytes.try_into().expect("slice is N bytes"))
    }

    fn u8(&mut self, what: &'static str) -> Result<u8, Kv3Error> {
        Ok(self.take::<1>(what)?[0])
    }

    fn u16(&mut self, what: &'static str) -> Result<u16, Kv3Error> {
        Ok(u16::from_le_bytes(self.take(what)?))
    }

    fn i32(&mut self, what: &'static str) -> Result<i32, Kv3Error> {
        Ok(i32::from_le_bytes(self.take(what)?))
    }

    fn u32(&mut self, what: &'static str) -> Result<u32, Kv3Error> {
        Ok(u32::from_le_bytes(self.take(what)?))
    }

    fn count(&mut self, what: &'static str) -> Result<usize, Kv3Error> {
        usize::try_from(self.i32(what)?).map_err(|_| Kv3Error::Truncated(what))
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// `n` bytes starting at the next multiple of `align` from the lane's own origin.
    fn aligned_slice(&mut self, n: usize, align: usize) -> Result<&'a [u8], Kv3Error> {
        if n == 0 {
            return Ok(&[]);
        }
        let start = self.pos.div_ceil(align) * align;
        let slice = self
            .data
            .get(start..start + n)
            .ok_or(Kv3Error::Truncated("lane"))?;
        self.pos = start + n;
        Ok(slice)
    }

    fn cstr(&mut self) -> Result<String, Kv3Error> {
        let rest = &self.data[self.pos..];
        let nul = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(Kv3Error::Truncated("string table"))?;
        self.pos += nul + 1;
        Ok(String::from_utf8_lossy(&rest[..nul]).into_owned())
    }
}

#[derive(Default)]
struct Lanes<'a> {
    bytes1: Option<Lane<'a>>,
    bytes2: Option<Lane<'a>>,
    bytes4: Option<Lane<'a>>,
    bytes8: Option<Lane<'a>>,
}

impl<'a> Lanes<'a> {
    /// Cuts the four aligned lanes out of a buffer, `bytes1` first, in the compiler's order.
    fn cut(
        buffer: &mut Lane<'a>,
        counts: [usize; 4],
        align_empty_8: bool,
    ) -> Result<Lanes<'a>, Kv3Error> {
        let [c1, c2, c4, c8] = counts;
        let lanes = Lanes {
            bytes1: Some(Lane::new(buffer.aligned_slice(c1, 1)?)),
            bytes2: Some(Lane::new(buffer.aligned_slice(c2 * 2, 2)?)),
            bytes4: Some(Lane::new(buffer.aligned_slice(c4 * 4, 4)?)),
            bytes8: Some(Lane::new(buffer.aligned_slice(c8 * 8, 8)?)),
        };
        if c8 == 0 && align_empty_8 {
            buffer.pos = buffer.pos.div_ceil(8) * 8;
        }
        Ok(lanes)
    }

    fn lane(&mut self, width: usize) -> &mut Lane<'a> {
        match width {
            1 => &mut self.bytes1,
            2 => &mut self.bytes2,
            4 => &mut self.bytes4,
            _ => &mut self.bytes8,
        }
        .as_mut()
        .expect("every lane is cut, possibly empty")
    }
}

struct Reader<'a> {
    version: u8,
    strings: Vec<String>,
    types: Lane<'a>,
    main: Lanes<'a>,
    /// Version 5 keeps `ARRAY_TYPE_AUXILIARY_BUFFER` elements in the first buffer's lanes.
    aux: Option<Lanes<'a>>,
    object_lengths: Option<Lane<'a>>,
}

mod node {
    pub const NULL: u8 = 1;
    pub const BOOLEAN: u8 = 2;
    pub const INT64: u8 = 3;
    pub const UINT64: u8 = 4;
    pub const DOUBLE: u8 = 5;
    pub const STRING: u8 = 6;
    pub const BINARY_BLOB: u8 = 7;
    pub const ARRAY: u8 = 8;
    pub const OBJECT: u8 = 9;
    pub const ARRAY_TYPED: u8 = 10;
    pub const INT32: u8 = 11;
    pub const UINT32: u8 = 12;
    pub const BOOLEAN_TRUE: u8 = 13;
    pub const BOOLEAN_FALSE: u8 = 14;
    pub const INT64_ZERO: u8 = 15;
    pub const INT64_ONE: u8 = 16;
    pub const DOUBLE_ZERO: u8 = 17;
    pub const DOUBLE_ONE: u8 = 18;
    pub const FLOAT: u8 = 19;
    pub const INT16: u8 = 20;
    pub const UINT16: u8 = 21;
    pub const INT8: u8 = 22;
    pub const UINT8: u8 = 23;
    pub const ARRAY_TYPE_BYTE_LENGTH: u8 = 24;
    pub const ARRAY_TYPE_AUXILIARY_BUFFER: u8 = 25;
}

/// Parses a binary KV3 blob (a `LaCo`, `RED2` or `DATA` block's bytes).
pub fn parse(bytes: &[u8]) -> Result<Document, Kv3Error> {
    let mut h = Lane::new(bytes);
    let magic = h.u32("magic")?;
    if magic == LEGACY_MAGIC {
        return parse_legacy(&mut h);
    }
    if magic & 0xFFFF_FF00 != MAGIC {
        return Err(Kv3Error::Magic(magic));
    }
    let version = (magic & 0xFF) as u8;
    if !(4..=5).contains(&version) {
        return Err(Kv3Error::Version(version));
    }
    h.take::<16>("format id")?;
    let method = h.u32("compression method")?;
    if method > LZ4 {
        return Err(Kv3Error::Compression(method));
    }
    h.u16("dictionary id")?;
    h.u16("frame size")?;
    let c1 = h.count("bytes1 count")?;
    let c4 = h.count("bytes4 count")?;
    let c8 = h.count("bytes8 count")?;
    let count_types = h.count("types count")?;
    h.u16("object count")?;
    h.u16("array count")?;
    let uncompressed_total = h.count("uncompressed size")?;
    let compressed_total = h.count("compressed size")?;
    let blocks = h.count("block count")?;
    h.count("blob bytes")?;
    let c2 = h.count("bytes2 count")?;
    let block_sizes_bytes = h.count("block sizes")?;
    if blocks > 0 {
        return Err(Kv3Error::Blobs);
    }

    let (u1, cp1, u2, cp2, counts2) = if version >= 5 {
        let u1 = h.count("buffer 1 size")?;
        let cp1 = h.count("buffer 1 compressed size")?;
        let u2 = h.count("buffer 2 size")?;
        let cp2 = h.count("buffer 2 compressed size")?;
        let counts2 = [
            h.count("buffer 2 bytes1")?,
            h.count("buffer 2 bytes2")?,
            h.count("buffer 2 bytes4")?,
            h.count("buffer 2 bytes8")?,
        ];
        h.count("node count")?;
        let objects2 = h.count("buffer 2 objects")?;
        h.count("buffer 2 arrays")?;
        h.count("element count")?;
        (u1, cp1, u2, cp2, Some((counts2, objects2)))
    } else {
        (uncompressed_total, compressed_total, 0, 0, None)
    };

    let mut read_buffer = |uncompressed: usize, compressed: usize| -> Result<Vec<u8>, Kv3Error> {
        if method == LZ4 {
            lz4_decode(h.take_slice(compressed)?, uncompressed)
        } else {
            Ok(h.take_slice(uncompressed)?.to_vec())
        }
    };
    let buffer1 = read_buffer(u1, cp1)?;
    let buffer2 = read_buffer(u2, cp2)?;

    let mut b1 = Lane::new(&buffer1);
    let mut lanes1 = Lanes::cut(&mut b1, [c1, c2, c4, c8], version < 5)?;
    let count_strings = lanes1.lane(4).count("string count")?;
    let mut strings = Vec::with_capacity(count_strings);

    let doc = match counts2 {
        Some((counts2, objects2)) => {
            for _ in 0..count_strings {
                strings.push(lanes1.lane(1).cstr()?);
            }
            let mut b2 = Lane::new(&buffer2);
            let object_lengths = Lane::new(b2.aligned_slice(objects2 * 4, 1)?);
            let main = Lanes::cut(&mut b2, counts2, false)?;
            let types = Lane::new(b2.aligned_slice(count_types, 1)?);
            let trailer = b2.u32("trailer")?;
            if trailer != TRAILER {
                return Err(Kv3Error::Trailer(trailer));
            }
            if b2.remaining() != block_sizes_bytes {
                return Err(Kv3Error::Truncated("buffer 2 tail"));
            }
            Reader {
                version,
                strings,
                types,
                main,
                aux: Some(lanes1),
                object_lengths: Some(object_lengths),
            }
            .document()
        }
        None => {
            let strings_start = b1.pos;
            for _ in 0..count_strings {
                strings.push(b1.cstr()?);
            }
            let types_len = count_types
                .checked_sub(b1.pos - strings_start)
                .ok_or(Kv3Error::Truncated("types"))?;
            let types = Lane::new(b1.aligned_slice(types_len, 1)?);
            let trailer = b1.u32("trailer")?;
            if trailer != TRAILER {
                return Err(Kv3Error::Trailer(trailer));
            }
            Reader {
                version,
                strings,
                types,
                main: lanes1,
                aux: None,
                object_lengths: None,
            }
            .document()
        }
    }?;
    Ok(doc)
}

impl<'a> Lane<'a> {
    fn take_slice(&mut self, n: usize) -> Result<&'a [u8], Kv3Error> {
        let slice = self
            .data
            .get(self.pos..self.pos + n)
            .ok_or(Kv3Error::Truncated("buffer"))?;
        self.pos += n;
        Ok(slice)
    }

    fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }
}

/// After the magic: a 16-byte encoding id, a 16-byte format id, then the body (a u32
/// string count, NUL-terminated strings, and the root node with every value inline).
fn parse_legacy(h: &mut Lane) -> Result<Document, Kv3Error> {
    let encoding = h.take::<16>("encoding")?;
    h.take::<16>("format")?;
    let body = match encoding {
        LEGACY_UNCOMPRESSED => h.rest().to_vec(),
        LEGACY_LZ4 => {
            let size = h.count("uncompressed size")?;
            lz4_decode(h.rest(), size)?
        }
        other => {
            let hex: String = other.iter().map(|b| format!("{b:02x}")).collect();
            return Err(Kv3Error::Encoding(hex));
        }
    };
    let mut lane = Lane::new(&body);
    let count = lane.count("string count")?;
    let mut strings = Vec::with_capacity(count.min(1 << 16));
    for _ in 0..count {
        strings.push(lane.cstr()?);
    }
    let mut reader = Legacy { lane, strings };
    let kind = reader.node_type()?;
    let root = reader.value(kind)?;
    Ok(Document { version: 1, root })
}

struct Legacy<'a> {
    lane: Lane<'a>,
    strings: Vec<String>,
}

impl Legacy<'_> {
    fn node_type(&mut self) -> Result<u8, Kv3Error> {
        let byte = self.lane.u8("node type")?;
        if byte & 0x80 != 0 {
            self.lane.u8("node flag")?;
        }
        Ok(byte & 0x7F)
    }

    fn string(&self, id: i32) -> String {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.strings.get(i))
            .cloned()
            .unwrap_or_default()
    }

    fn value(&mut self, kind: u8) -> Result<Value, Kv3Error> {
        Ok(match kind {
            node::NULL => Value::Null,
            node::BOOLEAN_TRUE => Value::Bool(true),
            node::BOOLEAN_FALSE => Value::Bool(false),
            node::INT64_ZERO => Value::Int(0),
            node::INT64_ONE => Value::Int(1),
            node::DOUBLE_ZERO => Value::Float(0.0),
            node::DOUBLE_ONE => Value::Float(1.0),
            node::BOOLEAN => Value::Bool(self.lane.u8("bool")? != 0),
            node::INT32 => Value::Int(i64::from(self.lane.i32("int32")?)),
            node::UINT32 => Value::UInt(u64::from(self.lane.u32("uint32")?)),
            node::FLOAT => Value::Float(f64::from(f32::from_le_bytes(self.lane.take("float")?))),
            node::INT64 => Value::Int(i64::from_le_bytes(self.lane.take("int64")?)),
            node::UINT64 => Value::UInt(u64::from_le_bytes(self.lane.take("uint64")?)),
            node::DOUBLE => Value::Float(f64::from_le_bytes(self.lane.take("double")?)),
            node::STRING => {
                let id = self.lane.i32("string id")?;
                Value::Str(self.string(id))
            }
            node::BINARY_BLOB => return Err(Kv3Error::Blobs),
            node::ARRAY => {
                let n = self.lane.count("array length")?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    let kind = self.node_type()?;
                    items.push(self.value(kind)?);
                }
                Value::Array(items)
            }
            node::ARRAY_TYPED => {
                let n = self.lane.count("typed array length")?;
                let element = self.node_type()?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    items.push(self.value(element)?);
                }
                Value::Array(items)
            }
            node::OBJECT => {
                let n = self.lane.count("object length")?;
                let mut members = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    let id = self.lane.i32("member name")?;
                    let name = self.string(id);
                    let kind = self.node_type()?;
                    members.push((name, self.value(kind)?));
                }
                Value::Object(members)
            }
            other => return Err(Kv3Error::NodeType(other)),
        })
    }
}

impl<'a> Reader<'a> {
    fn document(mut self) -> Result<Document, Kv3Error> {
        let (kind, _) = self.node_type()?;
        let root = self.value(kind, false)?;
        Ok(Document {
            version: self.version,
            root,
        })
    }

    /// The next type byte; flags (bit 7) and the unused bit 6 byte are skipped.
    fn node_type(&mut self) -> Result<(u8, u8), Kv3Error> {
        let byte = self.types.u8("node type")?;
        let flag = if byte & 0x80 != 0 {
            self.types.u8("node flag")?
        } else {
            0
        };
        if byte & 0x40 != 0 {
            self.types.u8("node extra")?;
        }
        Ok((byte & 0x3F, flag))
    }

    fn string(&self, id: i32) -> String {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.strings.get(i))
            .cloned()
            .unwrap_or_default()
    }

    fn lanes(&mut self, aux: bool) -> &mut Lanes<'a> {
        match (&mut self.aux, aux) {
            (Some(lanes), true) => lanes,
            _ => &mut self.main,
        }
    }

    fn value(&mut self, kind: u8, aux: bool) -> Result<Value, Kv3Error> {
        Ok(match kind {
            node::NULL => Value::Null,
            node::BOOLEAN_TRUE => Value::Bool(true),
            node::BOOLEAN_FALSE => Value::Bool(false),
            node::INT64_ZERO => Value::Int(0),
            node::INT64_ONE => Value::Int(1),
            node::DOUBLE_ZERO => Value::Float(0.0),
            node::DOUBLE_ONE => Value::Float(1.0),
            node::BOOLEAN => Value::Bool(self.lanes(aux).lane(1).u8("bool")? != 0),
            node::INT8 => Value::Int(i64::from(self.lanes(aux).lane(1).u8("int8")? as i8)),
            node::UINT8 => Value::UInt(u64::from(self.lanes(aux).lane(1).u8("uint8")?)),
            node::INT16 => Value::Int(i64::from(self.lanes(aux).lane(2).u16("int16")? as i16)),
            node::UINT16 => Value::UInt(u64::from(self.lanes(aux).lane(2).u16("uint16")?)),
            node::INT32 => Value::Int(i64::from(self.lanes(aux).lane(4).i32("int32")?)),
            node::UINT32 => Value::UInt(u64::from(self.lanes(aux).lane(4).u32("uint32")?)),
            node::FLOAT => Value::Float(f64::from(f32::from_le_bytes(
                self.lanes(aux).lane(4).take("float")?,
            ))),
            node::INT64 => Value::Int(i64::from_le_bytes(self.lanes(aux).lane(8).take("int64")?)),
            node::UINT64 => {
                Value::UInt(u64::from_le_bytes(self.lanes(aux).lane(8).take("uint64")?))
            }
            node::DOUBLE => {
                Value::Float(f64::from_le_bytes(self.lanes(aux).lane(8).take("double")?))
            }
            node::STRING => {
                let id = self.main.lane(4).i32("string id")?;
                Value::Str(self.string(id))
            }
            node::BINARY_BLOB => return Err(Kv3Error::Blobs),
            node::ARRAY => {
                let n = self.main.lane(4).count("array length")?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    let (kind, _) = self.node_type()?;
                    items.push(self.value(kind, false)?);
                }
                Value::Array(items)
            }
            node::ARRAY_TYPED
            | node::ARRAY_TYPE_BYTE_LENGTH
            | node::ARRAY_TYPE_AUXILIARY_BUFFER => {
                let n = if kind == node::ARRAY_TYPED {
                    self.main.lane(4).count("typed array length")?
                } else {
                    usize::from(self.main.lane(1).u8("typed array length")?)
                };
                let (element, _) = self.node_type()?;
                let from_aux = kind == node::ARRAY_TYPE_AUXILIARY_BUFFER;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    items.push(self.value(element, from_aux)?);
                }
                Value::Array(items)
            }
            node::OBJECT => {
                let n = match &mut self.object_lengths {
                    Some(lane) => lane.count("object length")?,
                    None => self.main.lane(4).count("object length")?,
                };
                let mut members = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    let (kind, _) = self.node_type()?;
                    let id = self.main.lane(4).i32("member name")?;
                    let name = self.string(id);
                    members.push((name, self.value(kind, false)?));
                }
                Value::Object(members)
            }
            other => return Err(Kv3Error::NodeType(other)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::resource::Resource;

    const TOP_BAR: &[u8] = include_bytes!("../../tests/fixtures/hud/top_bar_vanilla.vxml_c");
    const MINIMAP: &[u8] = include_bytes!("../../tests/fixtures/hud/hud_minimap_vanilla.vxml_c");

    fn laco(resource: &[u8]) -> Vec<u8> {
        Resource::parse(resource)
            .unwrap()
            .block(b"LaCo")
            .expect("LaCo block")
            .data
            .clone()
    }

    fn count(value: &Value, etype: &str) -> usize {
        match value {
            Value::Object(members) => {
                let own = usize::from(value.get("eType").and_then(Value::as_str) == Some(etype));
                own + members.iter().map(|(_, v)| count(v, etype)).sum::<usize>()
            }
            Value::Array(items) => items.iter().map(|v| count(v, etype)).sum(),
            _ => 0,
        }
    }

    #[test]
    fn lz4_literals_and_matches() {
        assert_eq!(lz4_decode(&[0x30, b'a', b'b', b'c'], 3).unwrap(), b"abc");
        let repeated = [0x35, b'a', b'b', b'c', 3, 0];
        assert_eq!(lz4_decode(&repeated, 12).unwrap(), b"abcabcabcabc");
        let long = [0x3F, b'x', b'y', b'z', 1, 0, 0x00, 0x10, b'!'];
        let out = lz4_decode(&long, 3 + 19 + 1).unwrap();
        assert_eq!(&out[..3], b"xyz");
        assert!(out[3..22].iter().all(|&b| b == b'z'));
        assert_eq!(out[22], b'!');
        assert_eq!(lz4_decode(&[0x30, b'a'], 3), Err(Kv3Error::Lz4));
        assert_eq!(lz4_decode(&[0x05, 9, 0], 4), Err(Kv3Error::Lz4));
        assert_eq!(lz4_decode(&[0x30, b'a', b'b', b'c'], 4), Err(Kv3Error::Lz4));
    }

    #[test]
    fn decodes_the_vanilla_top_bar_layout() {
        let doc = parse(&laco(TOP_BAR)).unwrap();
        assert_eq!(doc.version, 5);
        let root = doc
            .root
            .get("m_AST")
            .and_then(|a| a.get("m_pRoot"))
            .unwrap();
        assert_eq!(root.get("eType").and_then(Value::as_str), Some("ROOT"));
        let children = root.get("vecChildren").and_then(Value::as_array).unwrap();
        assert_eq!(children.len(), 2, "styles and the CitadelHudTopBar panel");
        assert_eq!(
            children[0].get("sourceLineColumn"),
            Some(&Value::Array(vec![Value::Int(3), Value::Int(3)]))
        );
        let first_include = children[0]
            .get("vecChildren")
            .and_then(Value::as_array)
            .unwrap()[0]
            .get("child")
            .unwrap();
        assert_eq!(
            first_include.get("name").and_then(Value::as_str),
            Some("panorama/styles/citadel_base_styles.vcss")
        );
        assert_eq!(
            first_include.get("eType").and_then(Value::as_str),
            Some("REFERENCE_COMPILED")
        );
        assert_eq!(count(&doc.root, "INCLUDE"), 4);
        assert_eq!(count(&doc.root, "SCRIPTS"), 0);
        assert!(count(&doc.root, "PANEL") > 60);
        assert_eq!(
            count(&doc.root, "PANEL_ATTRIBUTE"),
            count(&doc.root, "PANEL_ATTRIBUTE_VALUE")
        );
    }

    #[test]
    fn decodes_the_vanilla_minimap_layout() {
        let doc = parse(&laco(MINIMAP)).unwrap();
        assert_eq!(
            count(&doc.root, "SNIPPET"),
            count(&doc.root, "SNIPPET").max(1)
        );
        assert!(count(&doc.root, "PANEL") > 20);
    }

    #[test]
    fn errors() {
        assert_eq!(parse(&[0; 4]), Err(Kv3Error::Magic(0)));
        let mut v3 = laco(TOP_BAR);
        v3[0] = 3;
        assert_eq!(parse(&v3), Err(Kv3Error::Version(3)));
        let mut zstd = laco(TOP_BAR);
        zstd[20] = 2;
        assert_eq!(parse(&zstd), Err(Kv3Error::Compression(2)));
        let cut = &laco(TOP_BAR)[..200];
        assert!(matches!(
            parse(cut),
            Err(Kv3Error::Truncated(_)) | Err(Kv3Error::Lz4)
        ));
    }

    /// `VKV\x03`, the given encoding, a zero format id, then `body`.
    fn legacy(encoding: [u8; 16], body: &[u8]) -> Vec<u8> {
        let mut out = LEGACY_MAGIC.to_le_bytes().to_vec();
        out.extend_from_slice(&encoding);
        out.extend_from_slice(&[0; 16]);
        out.extend_from_slice(body);
        out
    }

    /// A string table and one root object holding every inline value kind.
    fn legacy_body() -> Vec<u8> {
        let mut b = Vec::new();
        let strings = ["eType", "ROOT", "vecChildren", "name", "x"];
        b.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        for s in strings {
            b.extend_from_slice(s.as_bytes());
            b.push(0);
        }
        b.push(node::OBJECT);
        b.extend_from_slice(&3i32.to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes());
        b.extend_from_slice(&[node::STRING | 0x80, 0x01]);
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&2i32.to_le_bytes());
        b.push(node::ARRAY);
        b.extend_from_slice(&4i32.to_le_bytes());
        b.push(node::INT32);
        b.extend_from_slice(&7i32.to_le_bytes());
        b.push(node::BOOLEAN_TRUE);
        b.push(node::ARRAY_TYPED);
        b.extend_from_slice(&2i32.to_le_bytes());
        b.push(node::DOUBLE);
        b.extend_from_slice(&1.5f64.to_le_bytes());
        b.extend_from_slice(&2.5f64.to_le_bytes());
        b.push(node::OBJECT);
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&3i32.to_le_bytes());
        b.push(node::INT64);
        b.extend_from_slice(&(-9i64).to_le_bytes());
        b.extend_from_slice(&4i32.to_le_bytes());
        b.push(node::NULL);
        b
    }

    /// One LZ4 sequence of literals only.
    fn lz4_literals(data: &[u8]) -> Vec<u8> {
        let n = data.len();
        let mut out = vec![(n.min(15) << 4) as u8];
        if n >= 15 {
            let mut rest = n - 15;
            while rest >= 255 {
                out.push(255);
                rest -= 255;
            }
            out.push(rest as u8);
        }
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn decodes_the_legacy_inline_format() {
        let body = legacy_body();
        let doc = parse(&legacy(LEGACY_UNCOMPRESSED, &body)).unwrap();
        assert_eq!(doc.version, 1);
        assert_eq!(doc.root.get("eType").and_then(Value::as_str), Some("ROOT"));
        let children = doc
            .root
            .get("vecChildren")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(children[0], Value::Int(7));
        assert_eq!(children[1], Value::Bool(true));
        assert_eq!(
            children[2],
            Value::Array(vec![Value::Float(1.5), Value::Float(2.5)])
        );
        assert_eq!(children[3].get("name"), Some(&Value::Int(-9)));
        assert_eq!(doc.root.get("x"), Some(&Value::Null));

        let mut lz4 = (body.len() as u32).to_le_bytes().to_vec();
        lz4.extend(lz4_literals(&body));
        assert_eq!(parse(&legacy(LEGACY_LZ4, &lz4)).unwrap(), doc);

        let block = [
            0x46, 0x1A, 0x79, 0x95, 0xBC, 0x95, 0x6C, 0x4F, 0xA7, 0x0B, 0x05, 0xBC, 0xA1, 0xB7,
            0xDF, 0xD2,
        ];
        assert!(matches!(
            parse(&legacy(block, &body)),
            Err(Kv3Error::Encoding(_))
        ));
        assert!(matches!(
            parse(&legacy(LEGACY_UNCOMPRESSED, &body[..body.len() - 3])),
            Err(Kv3Error::Truncated(_))
        ));
    }

    /// `DEADTUNE_KV3_SAMPLES=<dir>` points at compiled layouts kept outside the repo
    /// (mod files that can't be redistributed); every one must decode.
    #[test]
    fn decodes_every_local_sample() {
        let Ok(dir) = std::env::var("DEADTUNE_KV3_SAMPLES") else {
            return;
        };
        let mut seen = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "vxml_c") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let Some(block) = Resource::parse(&bytes).unwrap().block(b"LaCo").cloned() else {
                continue;
            };
            let doc = parse(&block.data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(count(&doc.root, "PANEL") > 0, "{}", path.display());
            seen += 1;
        }
        assert!(seen > 0, "no compiled layouts in the sample dir");
    }
}
