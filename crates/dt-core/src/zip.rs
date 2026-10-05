//! Just enough of the zip format to pull files out of a mod archive or an image collection,
//! and to write one (the UI images export): stored or deflated entries, no zip64, no encryption.

use std::io::{self, Read, Write};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ZipError {
    #[error("not a zip archive")]
    NotZip,
    #[error("the zip archive is damaged")]
    Damaged,
    #[error("no {0} in the zip archive")]
    Missing(String),
    #[error("more than one {0} in the zip archive")]
    Ambiguous(String),
    #[error("{0} uses an unsupported zip feature")]
    Unsupported(String),
}

const EOCD: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

fn u16_at(b: &[u8], at: usize) -> Result<u16, ZipError> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(ZipError::Damaged)
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ZipError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(ZipError::Damaged)
}

struct Entry<'a> {
    name: &'a str,
    flags: u16,
    method: u16,
    crc: u32,
    compressed: usize,
    size: usize,
    local_offset: usize,
}

fn entries(zip: &[u8]) -> Result<Vec<Entry<'_>>, ZipError> {
    let search_from = zip.len().saturating_sub(22 + u16::MAX as usize);
    let eocd = (search_from..=zip.len().saturating_sub(22))
        .rev()
        .find(|&at| u32_at(zip, at) == Ok(EOCD))
        .ok_or(ZipError::NotZip)?;
    let count = u16_at(zip, eocd + 10)? as usize;
    let mut at = u32_at(zip, eocd + 16)? as usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(zip, at)? != CENTRAL {
            return Err(ZipError::Damaged);
        }
        let name_len = u16_at(zip, at + 28)? as usize;
        let skip = name_len + u16_at(zip, at + 30)? as usize + u16_at(zip, at + 32)? as usize;
        let name = zip
            .get(at + 46..at + 46 + name_len)
            .ok_or(ZipError::Damaged)?;
        out.push(Entry {
            name: std::str::from_utf8(name).map_err(|_| ZipError::Damaged)?,
            flags: u16_at(zip, at + 8)?,
            method: u16_at(zip, at + 10)?,
            crc: u32_at(zip, at + 16)?,
            compressed: u32_at(zip, at + 20)? as usize,
            size: u32_at(zip, at + 24)? as usize,
            local_offset: u32_at(zip, at + 42)? as usize,
        });
        at += 46 + skip;
    }
    Ok(out)
}

/// The one entry whose file name (ignoring folders and case) is `file_name`.
pub fn extract(zip: &[u8], file_name: &str) -> Result<Vec<u8>, ZipError> {
    let all = entries(zip)?;
    let mut matches = all.iter().filter(|e| {
        e.name
            .rsplit('/')
            .next()
            .is_some_and(|base| base.eq_ignore_ascii_case(file_name))
    });
    let entry = matches
        .next()
        .ok_or_else(|| ZipError::Missing(file_name.to_string()))?;
    if matches.next().is_some() {
        return Err(ZipError::Ambiguous(file_name.to_string()));
    }
    read(zip, entry)
}

/// Every file in the archive with its full name, folders left out.
pub fn files(zip: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ZipError> {
    entries(zip)?
        .iter()
        .filter(|e| !e.name.ends_with('/'))
        .map(|e| Ok((e.name.to_string(), read(zip, e)?)))
        .collect()
}

fn read(zip: &[u8], entry: &Entry<'_>) -> Result<Vec<u8>, ZipError> {
    let unsupported = || ZipError::Unsupported(entry.name.to_string());
    if entry.flags & 1 != 0 || entry.size == u32::MAX as usize {
        return Err(unsupported());
    }
    let local = entry.local_offset;
    if u32_at(zip, local)? != LOCAL {
        return Err(ZipError::Damaged);
    }
    let start = local + 30 + u16_at(zip, local + 26)? as usize + u16_at(zip, local + 28)? as usize;
    let data = zip
        .get(start..start + entry.compressed)
        .ok_or(ZipError::Damaged)?;
    let bytes = match entry.method {
        0 => data.to_vec(),
        8 => {
            let mut out = Vec::with_capacity(entry.size);
            flate2::read::DeflateDecoder::new(data)
                .take(entry.size as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|_| ZipError::Damaged)?;
            out
        }
        _ => return Err(unsupported()),
    };
    let mut crc = flate2::Crc::new();
    crc.update(&bytes);
    if bytes.len() != entry.size || crc.sum() != entry.crc {
        return Err(ZipError::Damaged);
    }
    Ok(bytes)
}

pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(&LOCAL.to_le_bytes())
}

/// Streams entries into a zip archive: each one is deflated unless that does not make it
/// smaller (PNGs), names are UTF-8, every entry carries the same timestamp.
pub struct ZipWriter<W: Write> {
    out: W,
    offset: u64,
    central: Vec<u8>,
    count: u16,
    time: u16,
    date: u16,
}

fn too_big() -> io::Error {
    io::Error::other("the zip archive would be over 4 GB or 65535 files")
}

impl<W: Write> ZipWriter<W> {
    pub fn new(out: W, modified: chrono::NaiveDateTime) -> ZipWriter<W> {
        use chrono::{Datelike, Timelike};
        let year = (modified.year().clamp(1980, 2107) - 1980) as u16;
        ZipWriter {
            out,
            offset: 0,
            central: Vec::new(),
            count: 0,
            time: (modified.hour() as u16) << 11
                | (modified.minute() as u16) << 5
                | (modified.second() as u16 / 2),
            date: year << 9 | (modified.month() as u16) << 5 | modified.day() as u16,
        }
    }

    pub fn add(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let mut deflater =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        deflater.write_all(bytes)?;
        let deflated = deflater.finish()?;
        let (method, data): (u16, &[u8]) = if deflated.len() < bytes.len() {
            (8, &deflated)
        } else {
            (0, bytes)
        };
        let mut crc = flate2::Crc::new();
        crc.update(bytes);
        let size = u32::try_from(bytes.len()).map_err(|_| too_big())?;
        let compressed = data.len() as u32;
        let offset = u32::try_from(self.offset).map_err(|_| too_big())?;
        self.count = self.count.checked_add(1).ok_or_else(too_big)?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
        let common = |out: &mut Vec<u8>| {
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0x0800u16.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&self.time.to_le_bytes());
            out.extend_from_slice(&self.date.to_le_bytes());
            out.extend_from_slice(&crc.sum().to_le_bytes());
            out.extend_from_slice(&compressed.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
        };
        let mut local = LOCAL.to_le_bytes().to_vec();
        common(&mut local);
        local.extend_from_slice(name.as_bytes());
        let mut central = CENTRAL.to_le_bytes().to_vec();
        central.extend_from_slice(&20u16.to_le_bytes());
        common(&mut central);
        central.extend_from_slice(&[0; 10]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
        self.central.extend_from_slice(&central);
        self.out.write_all(&local)?;
        self.out.write_all(data)?;
        self.offset += (local.len() + data.len()) as u64;
        Ok(())
    }

    /// Writes the central directory and hands back the writer.
    pub fn finish(mut self) -> io::Result<W> {
        let start = u32::try_from(self.offset).map_err(|_| too_big())?;
        let size = self.central.len() as u32;
        self.out.write_all(&self.central)?;
        let mut end = EOCD.to_le_bytes().to_vec();
        end.extend_from_slice(&[0; 4]);
        end.extend_from_slice(&self.count.to_le_bytes());
        end.extend_from_slice(&self.count.to_le_bytes());
        end.extend_from_slice(&size.to_le_bytes());
        end.extend_from_slice(&start.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        self.out.write_all(&end)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GI: &[u8] = include_bytes!("../tests/fixtures/sidelock_standin/gameinfo.gi");
    const DEFLATED: &[u8] = include_bytes!("../tests/fixtures/sidelock_standin/cfg.zip");
    const NESTED_STORED: &[u8] =
        include_bytes!("../tests/fixtures/sidelock_standin/nested_stored.zip");

    #[test]
    fn extracts_a_deflated_entry() {
        assert_eq!(extract(DEFLATED, "gameinfo.gi").unwrap(), GI);
    }

    #[test]
    fn extracts_a_stored_entry_inside_a_folder_ignoring_case() {
        assert_eq!(extract(NESTED_STORED, "GameInfo.gi").unwrap(), GI);
        assert_eq!(extract(NESTED_STORED, "readme.txt").unwrap(), b"readme\n");
    }

    #[test]
    fn missing_entry_and_non_zip_input_are_errors() {
        assert_eq!(
            extract(DEFLATED, "video.txt"),
            Err(ZipError::Missing("video.txt".into()))
        );
        assert_eq!(extract(GI, "gameinfo.gi"), Err(ZipError::NotZip));
        assert_eq!(extract(b"", "gameinfo.gi"), Err(ZipError::NotZip));
        assert!(is_zip(DEFLATED) && !is_zip(GI));
    }

    #[test]
    fn corrupted_data_fails_the_crc_check() {
        let mut bad = NESTED_STORED.to_vec();
        let at = bad.windows(8).position(|w| w == b"\"GameInf").unwrap();
        bad[at + 1] = b'X';
        assert_eq!(extract(&bad, "gameinfo.gi"), Err(ZipError::Damaged));
    }

    #[test]
    fn written_archives_read_back_stored_and_deflated() {
        let when = chrono::NaiveDate::from_ymd_opt(2026, 10, 5)
            .unwrap()
            .and_hms_opt(14, 30, 10)
            .unwrap();
        let mut zip = ZipWriter::new(Vec::new(), when);
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let noise: Vec<u8> = (0..4096)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 24) as u8
            })
            .collect();
        zip.add("panorama/images/minimap/a_psd.png", &noise)
            .unwrap();
        zip.add("manifest.json", &b"{\"a\": 1}\n".repeat(200))
            .unwrap();
        zip.add("empty.txt", b"").unwrap();
        let bytes = zip.finish().unwrap();
        assert!(is_zip(&bytes));
        assert_eq!(extract(&bytes, "a_psd.png").unwrap(), noise);
        assert_eq!(
            extract(&bytes, "manifest.json").unwrap(),
            b"{\"a\": 1}\n".repeat(200)
        );
        assert_eq!(extract(&bytes, "empty.txt").unwrap(), b"");
        let names: Vec<(String, usize)> = files(&bytes)
            .unwrap()
            .into_iter()
            .map(|(name, data)| (name, data.len()))
            .collect();
        assert_eq!(
            names,
            [
                ("panorama/images/minimap/a_psd.png".to_string(), 4096),
                ("manifest.json".to_string(), 1800),
                ("empty.txt".to_string(), 0)
            ]
        );
        let all = entries(&bytes).unwrap();
        let methods: Vec<(&str, u16)> = all.iter().map(|e| (e.name, e.method)).collect();
        assert_eq!(
            methods,
            [
                ("panorama/images/minimap/a_psd.png", 0),
                ("manifest.json", 8),
                ("empty.txt", 0)
            ]
        );
        assert!(bytes.len() < 4096 + 1800, "the repeated text is deflated");
        let date = u16_at(&bytes, 12).unwrap();
        assert_eq!((date >> 9, (date >> 5) & 15, date & 31), (46, 10, 5));
    }

    #[test]
    fn truncated_archive_is_not_a_panic() {
        for cut in [10, DEFLATED.len() / 2, DEFLATED.len() - 1] {
            assert!(extract(&DEFLATED[..cut], "gameinfo.gi").is_err(), "{cut}");
        }
    }
}
