//! VPK v2 archives: read the game's split `pak01_dir.vpk` (+ `pak01_NNN.vpk`), and
//! write single-file addons where every entry's data follows the tree (index 0x7fff).
//! Format notes: research/hud/vpk-and-compiled-resources.md section 4.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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
        let name = stem(&self.dir_path)?;
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

/// Builds a single-file VPK v2. Deterministic: same input, same bytes. The MD5
/// section is zero-filled (QoL Lite ships this way and loads).
pub fn write(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();
    let mut entries = BTreeMap::new();
    for (path, bytes) in files {
        entries.insert(
            path.clone(),
            VpkEntry {
                crc: crc32(bytes),
                preload: Vec::new(),
                archive_index: EMBEDDED,
                offset: data.len() as u32,
                length: bytes.len() as u32,
            },
        );
        data.extend_from_slice(bytes);
    }
    let t = tree_bytes(&entries);
    let mut out = Vec::with_capacity(HEADER_V2 + t.len() + data.len() + 48);
    out.extend_from_slice(&header(&t, data.len()));
    out.extend_from_slice(&t);
    out.extend_from_slice(&data);
    out.extend_from_slice(&[0u8; 48]);
    out
}

fn header(tree: &[u8], data_len: usize) -> [u8; HEADER_V2] {
    let mut out = [0u8; HEADER_V2];
    for (i, v) in [SIGNATURE, 2, tree.len() as u32, data_len as u32, 0, 48, 0]
        .iter()
        .enumerate()
    {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// The directory tree: ext -> dir -> name, sorted, each name followed by its record.
fn tree_bytes(entries: &BTreeMap<String, VpkEntry>) -> Vec<u8> {
    type Dirs<'a> = BTreeMap<&'a str, BTreeMap<&'a str, &'a VpkEntry>>;
    let mut tree: BTreeMap<&str, Dirs> = BTreeMap::new();
    for (path, e) in entries {
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
            .insert(name, e);
    }
    let mut t: Vec<u8> = Vec::new();
    let cstr = |t: &mut Vec<u8>, s: &str| {
        t.extend_from_slice(s.as_bytes());
        t.push(0);
    };
    for (ext, dirs) in &tree {
        cstr(&mut t, ext);
        for (dir, names) in dirs {
            cstr(&mut t, dir);
            for (name, e) in names {
                cstr(&mut t, name);
                t.extend_from_slice(&e.crc.to_le_bytes());
                t.extend_from_slice(&(e.preload.len() as u16).to_le_bytes());
                t.extend_from_slice(&e.archive_index.to_le_bytes());
                t.extend_from_slice(&e.offset.to_le_bytes());
                t.extend_from_slice(&e.length.to_le_bytes());
                t.extend_from_slice(&TERMINATOR.to_le_bytes());
                t.extend_from_slice(&e.preload);
            }
            t.push(0);
        }
        t.push(0);
    }
    t.push(0);
    t
}

/// Streams entries to disk as they arrive, so a multi-gigabyte archive never sits in
/// memory. Data goes to `<stem>_000.vpk`, `<stem>_001.vpk`, ... (a new chunk once the
/// current one passes `chunk_limit`), and `finish` writes `<stem>_dir.vpk`. When
/// everything fit in one chunk, `finish` folds it into the dir file instead, which is
/// the single-file shape every known addon uses. Dropping the writer before `finish`
/// deletes what it wrote.
pub struct VpkWriter {
    dir_path: PathBuf,
    chunk_limit: u64,
    chunk: Option<File>,
    chunk_index: u16,
    chunk_len: u64,
    entries: BTreeMap<String, VpkEntry>,
    finished: bool,
}

impl VpkWriter {
    /// Removes any `<stem>_dir.vpk` and `<stem>_NNN.vpk` already at `dir_path` first,
    /// so a rerun never leaves stale chunks behind.
    pub fn create(dir_path: &Path, chunk_limit: u64) -> Result<VpkWriter, VpkError> {
        let stem = stem(dir_path)?;
        if let Some(parent) = dir_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
            for entry in std::fs::read_dir(parent)? {
                let entry = entry?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Some(rest) = name.strip_prefix(&stem) else {
                    continue;
                };
                let is_chunk = rest.len() == 8
                    && rest.starts_with('_')
                    && rest.ends_with(".vpk")
                    && rest[1..4].bytes().all(|b| b.is_ascii_digit());
                if is_chunk || rest == "_dir.vpk" {
                    std::fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(VpkWriter {
            dir_path: dir_path.to_path_buf(),
            chunk_limit,
            chunk: None,
            chunk_index: 0,
            chunk_len: 0,
            entries: BTreeMap::new(),
            finished: false,
        })
    }

    pub fn add(&mut self, path: &str, data: &[u8]) -> Result<(), VpkError> {
        use std::io::Write;
        if self.chunk.is_some() && self.chunk_len + data.len() as u64 > self.chunk_limit {
            self.chunk = None;
            self.chunk_index += 1;
            self.chunk_len = 0;
        }
        let file = match &mut self.chunk {
            Some(f) => f,
            None => self
                .chunk
                .insert(File::create(self.chunk_path(self.chunk_index))?),
        };
        file.write_all(data)?;
        self.entries.insert(
            path.to_string(),
            VpkEntry {
                crc: crc32(data),
                preload: Vec::new(),
                archive_index: self.chunk_index,
                offset: self.chunk_len as u32,
                length: data.len() as u32,
            },
        );
        self.chunk_len += data.len() as u64;
        Ok(())
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Writes the dir file and returns every file that makes up the archive.
    pub fn finish(mut self) -> Result<Vec<PathBuf>, VpkError> {
        use std::io::Write;
        let fold = self.chunk_index == 0;
        if fold {
            for e in self.entries.values_mut() {
                e.archive_index = EMBEDDED;
            }
        }
        let tree = tree_bytes(&self.entries);
        let mut dir = std::io::BufWriter::new(File::create(&self.dir_path)?);
        dir.write_all(&header(
            &tree,
            if fold { self.chunk_len as usize } else { 0 },
        ))?;
        dir.write_all(&tree)?;
        let mut files = vec![self.dir_path.clone()];
        if fold {
            let chunk = self.chunk_path(0);
            self.chunk = None;
            if self.chunk_len > 0 {
                std::io::copy(&mut File::open(&chunk)?, &mut dir)?;
            }
            std::fs::remove_file(chunk)?;
        } else {
            files.extend((0..=self.chunk_index).map(|i| self.chunk_path(i)));
        }
        dir.write_all(&[0u8; 48])?;
        dir.flush()?;
        self.finished = true;
        Ok(files)
    }

    fn chunk_path(&self, index: u16) -> PathBuf {
        let stem = stem(&self.dir_path).expect("validated in create");
        self.dir_path
            .with_file_name(format!("{stem}_{index:03}.vpk"))
    }
}

impl Drop for VpkWriter {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.chunk = None;
        let _ = std::fs::remove_file(&self.dir_path);
        for i in 0..=self.chunk_index {
            let _ = std::fs::remove_file(self.chunk_path(i));
        }
    }
}

fn stem(dir_path: &Path) -> Result<String, VpkError> {
    dir_path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix("_dir.vpk"))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| VpkError::BadHeader("dir file name must end in _dir.vpk".into()))
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

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn writer_folds_single_chunk_into_dir() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pak78_dir.vpk");
        std::fs::write(dir.path().join("pak78_003.vpk"), b"stale").unwrap();
        std::fs::write(dir.path().join("pak78_dir.vpk"), b"stale").unwrap();
        std::fs::write(dir.path().join("pak79_000.vpk"), b"other").unwrap();
        let files = sample();
        let mut w = VpkWriter::create(&p, 1 << 20).unwrap();
        assert_eq!(names(dir.path()), ["pak79_000.vpk"]);
        for (path, data) in &files {
            w.add(path, data).unwrap();
        }
        assert_eq!(w.entry_count(), files.len());
        let written = w.finish().unwrap();
        assert_eq!(written, std::slice::from_ref(&p));
        assert_eq!(names(dir.path()), ["pak78_dir.vpk", "pak79_000.vpk"]);
        assert_eq!(std::fs::read(&p).unwrap(), write(&files));
    }

    #[test]
    fn writer_splits_into_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pak78_dir.vpk");
        let files = sample();
        let mut w = VpkWriter::create(&p, 1000).unwrap();
        for (path, data) in &files {
            w.add(path, data).unwrap();
        }
        let written = w.finish().unwrap();
        assert_eq!(written.len(), 3, "{written:?}");
        assert_eq!(
            names(dir.path()),
            ["pak78_000.vpk", "pak78_001.vpk", "pak78_dir.vpk"]
        );
        let vpk = VpkDir::open(&p).unwrap();
        assert_eq!(vpk.entries.len(), files.len());
        for (path, data) in &files {
            assert_eq!(&vpk.read(path).unwrap(), data, "{path}");
        }
        assert!(vpk.entries["a/b/c/deep.vcss_c"].archive_index != EMBEDDED);
        assert_eq!(u32_at(&std::fs::read(&p).unwrap(), 12), 0);
    }

    #[test]
    fn writer_drop_removes_partial_output() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pak78_dir.vpk");
        let mut w = VpkWriter::create(&p, 10).unwrap();
        w.add("a.txt", b"hello world").unwrap();
        w.add("b.txt", b"second chunk").unwrap();
        assert_eq!(names(dir.path()), ["pak78_000.vpk", "pak78_001.vpk"]);
        drop(w);
        assert!(names(dir.path()).is_empty());
        assert!(VpkWriter::create(&dir.path().join("nope.vpk"), 10).is_err());
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
