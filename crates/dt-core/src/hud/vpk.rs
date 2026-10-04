//! VPK v2 archives: read the game's split `pak01_dir.vpk` (+ `pak01_NNN.vpk`), and
//! write single-file addons where every entry's data follows the tree (index 0x7fff).
//! Format notes: research/hud/vpk-and-compiled-resources.md section 4.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::crc32::crc32;

pub const SIGNATURE: u32 = 0x55AA_1234;
/// Archive index meaning "data is in the dir file, after the tree".
pub const EMBEDDED: u16 = 0x7fff;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VpkEntry {
    pub crc: u32,
    pub preload: Vec<u8>,
    pub archive_index: u16,
    pub offset: u32,
    pub length: u32,
}

/// Parsed directory of a `*_dir.vpk`. Paths use forward slashes and include the
/// extension (`panorama/styles/hud.vcss_c`).
#[derive(Clone, Debug)]
pub struct VpkDir {
    pub dir_path: PathBuf,
    pub entries: BTreeMap<String, VpkEntry>,
    /// Absolute offset of the embedded data section in the dir file (28 + tree size).
    data_start: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum VpkError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a VPK v2 file: {0}")]
    BadHeader(String),
    #[error("malformed tree: {0}")]
    BadTree(String),
    #[error("no entry {0}")]
    Missing(String),
    #[error("crc mismatch for {0}")]
    Crc(String),
}

const HEADER_V1: usize = 12;
const HEADER_V2: usize = 28;
const TERMINATOR: u16 = 0xFFFF;
/// Placeholder the format uses for an empty directory or extension.
const BLANK: &str = " ";

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn cstr(&mut self) -> Result<&'a str, VpkError> {
        let rest = &self.buf[self.pos..];
        let n = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| VpkError::BadTree("unterminated string".into()))?;
        self.pos += n + 1;
        std::str::from_utf8(&rest[..n]).map_err(|_| VpkError::BadTree("non-utf8 string".into()))
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], VpkError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| VpkError::BadTree("record past end of tree".into()))?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

impl VpkDir {
    pub fn open(dir_path: &Path) -> Result<VpkDir, VpkError> {
        // The tree is small; the data section is not read here.
        let mut file = File::open(dir_path)?;
        let mut head = [0u8; HEADER_V2];
        let mut got = 0;
        while got < head.len() {
            let n = file.read(&mut head[got..])?;
            if n == 0 {
                break;
            }
            got += n;
        }
        let header = parse_header(&head[..got])?;
        let total = header.0 + header.1;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![0u8; total];
        file.read_exact(&mut bytes)
            .map_err(|_| VpkError::BadTree("file shorter than header + tree".into()))?;
        VpkDir::parse(dir_path, &bytes)
    }

    /// Parses a dir file already in memory. `dir_path` is only used to find sibling
    /// `_NNN.vpk` archives on `read`.
    pub fn parse(dir_path: &Path, bytes: &[u8]) -> Result<VpkDir, VpkError> {
        let (hdr, tree_size) = parse_header(bytes)?;
        let tree_end = hdr
            .checked_add(tree_size)
            .filter(|&e| e <= bytes.len())
            .ok_or_else(|| VpkError::BadTree("tree extends past end of file".into()))?;
        let mut cur = Cursor {
            buf: &bytes[..tree_end],
            pos: hdr,
        };
        let mut entries = BTreeMap::new();
        loop {
            let ext = cur.cstr()?;
            if ext.is_empty() {
                break;
            }
            loop {
                let dir = cur.cstr()?;
                if dir.is_empty() {
                    break;
                }
                loop {
                    let name = cur.cstr()?;
                    if name.is_empty() {
                        break;
                    }
                    let rec = cur.take(18)?;
                    if u16_at(rec, 16) != TERMINATOR {
                        return Err(VpkError::BadTree(format!("bad terminator for {name}")));
                    }
                    let preload_len = u16_at(rec, 4) as usize;
                    let preload = cur.take(preload_len)?.to_vec();
                    let mut path = String::new();
                    if dir != BLANK {
                        path.push_str(dir);
                        path.push('/');
                    }
                    path.push_str(name);
                    if ext != BLANK {
                        path.push('.');
                        path.push_str(ext);
                    }
                    entries.insert(
                        path,
                        VpkEntry {
                            crc: u32_at(rec, 0),
                            preload,
                            archive_index: u16_at(rec, 6),
                            offset: u32_at(rec, 8),
                            length: u32_at(rec, 12),
                        },
                    );
                }
            }
        }
        Ok(VpkDir {
            dir_path: dir_path.to_path_buf(),
            entries,
            data_start: tree_end as u64,
        })
    }

    /// Full file bytes (preload + archive data), CRC-checked.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, VpkError> {
        let e = self
            .entries
            .get(path)
            .ok_or_else(|| VpkError::Missing(path.to_string()))?;
        let (file_path, start) = if e.archive_index == EMBEDDED {
            (self.dir_path.clone(), self.data_start + e.offset as u64)
        } else {
            (self.archive_path(e.archive_index)?, e.offset as u64)
        };
        let mut out = e.preload.clone();
        if e.length > 0 {
            let mut f = File::open(file_path)?;
            f.seek(SeekFrom::Start(start))?;
            let at = out.len();
            out.resize(at + e.length as usize, 0);
            f.read_exact(&mut out[at..])?;
        }
        if crc32(&out) != e.crc {
            return Err(VpkError::Crc(path.to_string()));
        }
        Ok(out)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    fn archive_path(&self, index: u16) -> Result<PathBuf, VpkError> {
        let name = self
            .dir_path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix("_dir.vpk"))
            .ok_or_else(|| VpkError::BadHeader("dir file name must end in _dir.vpk".into()))?;
        Ok(self
            .dir_path
            .with_file_name(format!("{name}_{index:03}.vpk")))
    }
}

/// Returns (header size, tree size).
fn parse_header(b: &[u8]) -> Result<(usize, usize), VpkError> {
    if b.len() < HEADER_V1 {
        return Err(VpkError::BadHeader("file too short".into()));
    }
    if u32_at(b, 0) != SIGNATURE {
        return Err(VpkError::BadHeader("bad signature".into()));
    }
    match u32_at(b, 4) {
        1 => Ok((HEADER_V1, u32_at(b, 8) as usize)),
        2 if b.len() >= HEADER_V2 => Ok((HEADER_V2, u32_at(b, 8) as usize)),
        2 => Err(VpkError::BadHeader("truncated v2 header".into())),
        v => Err(VpkError::BadHeader(format!("unsupported version {v}"))),
    }
}

/// Where an entry's bytes come from when writing.
#[derive(Clone, Debug)]
pub enum Data<'a> {
    Bytes(&'a [u8]),
    /// An entry of another archive, streamed through without being held with the rest.
    Copy {
        from: &'a VpkDir,
        path: &'a str,
    },
}

impl Data<'_> {
    fn crc_len(&self) -> Result<(u32, u32), VpkError> {
        match self {
            Data::Bytes(b) => Ok((crc32(b), b.len() as u32)),
            Data::Copy { from, path } => {
                let e = from
                    .entries
                    .get(*path)
                    .ok_or_else(|| VpkError::Missing(path.to_string()))?;
                Ok((e.crc, e.length + e.preload.len() as u32))
            }
        }
    }
}

/// Builds a single-file VPK v2. Deterministic: same input, same bytes. The MD5
/// section is zero-filled (QoL Lite ships this way and loads).
pub fn write(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let data: BTreeMap<String, Data> = files
        .iter()
        .map(|(p, b)| (p.clone(), Data::Bytes(b)))
        .collect();
    let mut out = Vec::new();
    write_with(&data, &mut out).expect("in-memory entries always resolve");
    out
}

/// [`write`] for entries that may be copied from another archive. The tree lists
/// every entry before any data, so copies are read twice: once for the tree (their
/// crc and length come from the source tree) and once while streaming.
pub fn write_with(files: &BTreeMap<String, Data>, out: &mut impl Write) -> Result<(), VpkError> {
    // ext -> dir -> [(name, data)], BTreeMap keeps tree order sorted.
    type Dirs<'a, 'b> = BTreeMap<&'a str, BTreeMap<&'a str, &'a Data<'b>>>;
    let mut tree: BTreeMap<&str, Dirs> = BTreeMap::new();
    for (path, data) in files {
        let (dir, file) = path.rsplit_once('/').unwrap_or((BLANK, path));
        let dir = if dir.is_empty() { BLANK } else { dir };
        let (name, ext) = match file.rsplit_once('.') {
            Some((n, e)) if !e.is_empty() => (n, e),
            _ => (file, BLANK),
        };
        tree.entry(ext)
            .or_default()
            .entry(dir)
            .or_default()
            .insert(name, data);
    }

    let mut t: Vec<u8> = Vec::new();
    let mut order: Vec<&Data> = Vec::new();
    let mut total: u64 = 0;
    let cstr = |t: &mut Vec<u8>, s: &str| {
        t.extend_from_slice(s.as_bytes());
        t.push(0);
    };
    for (ext, dirs) in &tree {
        cstr(&mut t, ext);
        for (dir, names) in dirs {
            cstr(&mut t, dir);
            for (name, data) in names {
                let (crc, len) = data.crc_len()?;
                let offset = u32::try_from(total)
                    .map_err(|_| VpkError::BadTree("data section exceeds 4 GiB".into()))?;
                cstr(&mut t, name);
                t.extend_from_slice(&crc.to_le_bytes());
                t.extend_from_slice(&0u16.to_le_bytes());
                t.extend_from_slice(&EMBEDDED.to_le_bytes());
                t.extend_from_slice(&offset.to_le_bytes());
                t.extend_from_slice(&len.to_le_bytes());
                t.extend_from_slice(&TERMINATOR.to_le_bytes());
                total += u64::from(len);
                order.push(data);
            }
            t.push(0);
        }
        t.push(0);
    }
    t.push(0);
    let data_len =
        u32::try_from(total).map_err(|_| VpkError::BadTree("data section exceeds 4 GiB".into()))?;

    for v in [SIGNATURE, 2, t.len() as u32, data_len, 0, 48, 0] {
        out.write_all(&v.to_le_bytes())?;
    }
    out.write_all(&t)?;
    for data in order {
        match data {
            Data::Bytes(b) => out.write_all(b)?,
            Data::Copy { from, path } => out.write_all(&from.read(path)?)?,
        }
    }
    out.write_all(&[0u8; 48])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BTreeMap<String, Vec<u8>> {
        let mut m = BTreeMap::new();
        m.insert(
            "panorama/styles/hud.vcss_c".to_string(),
            b"style bytes".to_vec(),
        );
        m.insert("panorama/layout/hud.vxml_c".to_string(), b"layout".to_vec());
        m.insert("root.txt".to_string(), b"root file".to_vec());
        m.insert("a/b/c/deep.vcss_c".to_string(), vec![7u8; 1000]);
        m.insert("noext".to_string(), b"no extension".to_vec());
        m.insert("empty.bin".to_string(), Vec::new());
        m
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pak_dir.vpk");
        let files = sample();
        let bytes = write(&files);
        assert_eq!(bytes, write(&files));
        std::fs::write(&p, &bytes).unwrap();
        for vpk in [
            VpkDir::open(&p).unwrap(),
            VpkDir::parse(&p, &bytes).unwrap(),
        ] {
            assert_eq!(vpk.entries.len(), files.len());
            for (path, data) in &files {
                assert!(vpk.contains(path), "{path}");
                assert_eq!(&vpk.read(path).unwrap(), data, "{path}");
            }
        }
    }

    #[test]
    fn copies_entries_from_another_archive_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let src_path = dir.path().join("src_dir.vpk");
        std::fs::write(&src_path, write(&sample())).unwrap();
        let src = VpkDir::open(&src_path).unwrap();
        let own = b"fresh".to_vec();
        let files = BTreeMap::from([
            (
                "a/b/c/deep.vcss_c".to_string(),
                Data::Copy {
                    from: &src,
                    path: "a/b/c/deep.vcss_c",
                },
            ),
            ("new/own.txt".to_string(), Data::Bytes(&own)),
            (
                "empty.bin".to_string(),
                Data::Copy {
                    from: &src,
                    path: "empty.bin",
                },
            ),
        ]);
        let mut out = Vec::new();
        write_with(&files, &mut out).unwrap();
        let expected = write(&BTreeMap::from([
            ("a/b/c/deep.vcss_c".to_string(), vec![7u8; 1000]),
            ("new/own.txt".to_string(), own),
            ("empty.bin".to_string(), Vec::new()),
        ]));
        assert_eq!(out, expected);
        let missing = BTreeMap::from([(
            "x".to_string(),
            Data::Copy {
                from: &src,
                path: "nope",
            },
        )]);
        assert!(matches!(
            write_with(&missing, &mut Vec::new()),
            Err(VpkError::Missing(_))
        ));
    }

    #[test]
    fn real_fixture() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hud/blur_pak97_dir.vpk");
        let vpk = VpkDir::open(&p).unwrap();
        assert_eq!(vpk.entries.len(), 2);
        assert!(vpk.contains("panorama/styles/citadel_base_styles.vcss_c"));
        for (path, e) in &vpk.entries {
            let data = vpk.read(path).unwrap();
            assert_eq!(data.len(), e.length as usize + e.preload.len());
        }
    }

    #[test]
    fn split_archive() {
        let dir = tempfile::tempdir().unwrap();
        let archive = b"xxxhello world";
        std::fs::write(dir.path().join("x_000.vpk"), archive).unwrap();
        let payload = &archive[3..];
        let mut t = Vec::new();
        t.extend_from_slice(b"txt\0dir\0name\0");
        t.extend_from_slice(&crc32(b"PRE hello world").to_le_bytes());
        t.extend_from_slice(&4u16.to_le_bytes());
        t.extend_from_slice(&0u16.to_le_bytes());
        t.extend_from_slice(&3u32.to_le_bytes());
        t.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        t.extend_from_slice(&TERMINATOR.to_le_bytes());
        t.extend_from_slice(b"PRE ");
        t.extend_from_slice(b"\0\0\0");
        let mut bytes = Vec::new();
        for v in [SIGNATURE, 2, t.len() as u32, 0, 0, 48, 0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&t);
        let p = dir.path().join("x_dir.vpk");
        std::fs::write(&p, &bytes).unwrap();
        let vpk = VpkDir::open(&p).unwrap();
        assert_eq!(vpk.read("dir/name.txt").unwrap(), b"PRE hello world");
    }

    #[test]
    fn crc_mismatch() {
        let files = BTreeMap::from([("a/b.txt".to_string(), b"data".to_vec())]);
        let mut bytes = write(&files);
        let tree_size = u32_at(&bytes, 8) as usize;
        bytes[HEADER_V2 + tree_size] ^= 0xFF;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("p_dir.vpk");
        std::fs::write(&p, &bytes).unwrap();
        let vpk = VpkDir::open(&p).unwrap();
        assert!(matches!(vpk.read("a/b.txt"), Err(VpkError::Crc(_))));
        assert!(matches!(vpk.read("nope"), Err(VpkError::Missing(_))));
    }

    #[test]
    fn malformed() {
        let p = Path::new("x_dir.vpk");
        assert!(matches!(
            VpkDir::parse(p, b"short"),
            Err(VpkError::BadHeader(_))
        ));
        let mut bad = write(&sample());
        bad[0] ^= 1;
        assert!(matches!(
            VpkDir::parse(p, &bad),
            Err(VpkError::BadHeader(_))
        ));
        let mut ver = write(&sample());
        ver[4] = 9;
        assert!(matches!(
            VpkDir::parse(p, &ver),
            Err(VpkError::BadHeader(_))
        ));
        let good = write(&sample());
        let tree_size = u32_at(&good, 8) as usize;
        assert!(matches!(
            VpkDir::parse(p, &good[..HEADER_V2 + tree_size - 5]),
            Err(VpkError::BadTree(_))
        ));
        let mut cut = good.clone();
        cut[8..12].copy_from_slice(&(tree_size as u32 - 3).to_le_bytes());
        assert!(VpkDir::parse(p, &cut).is_err());
    }
}
