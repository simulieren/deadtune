//! Prints the structure of every `.vcss_c` in a VPK or a loose file: blocks, the DATA
//! prefix and which CRC rule it satisfies, the image table and the text head.
//! `cargo run -p dt-core --example vcss_dump -- <file.vpk | file.vcss_c>`

use std::path::Path;

use dt_core::hud::crc32::crc32;
use dt_core::hud::resource::{self, Resource};
use dt_core::hud::vpk::VpkDir;

fn dump(label: &str, bytes: &[u8]) {
    println!("== {label}: {} bytes", bytes.len());
    let res = match Resource::parse(bytes) {
        Ok(r) => r,
        Err(e) => {
            println!("  parse error: {e}");
            return;
        }
    };
    println!(
        "  header v{} type v{}; round-trips: {}",
        res.header_version,
        res.type_version,
        res.to_bytes() == bytes
    );
    for b in &res.blocks {
        println!(
            "  block {} {} bytes",
            String::from_utf8_lossy(&b.name),
            b.data.len()
        );
    }
    let Some(data) = res.block(b"DATA") else {
        return;
    };
    let d = &data.data;
    let prefix = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
    let images = u16::from_le_bytes([d[4], d[5]]);
    match resource::style_text(&res) {
        Ok(text) => {
            let table = resource::image_table(&res).unwrap();
            println!(
                "  DATA prefix {prefix:08x}; images {images}; table {} bytes; text {} bytes",
                table.len(),
                text.len()
            );
            println!(
                "  crc32(text) {:08x}  crc32(table+text) {:08x}  crc32(after prefix) {:08x}  prefix^crc32(text) {:08x}",
                crc32(text.as_bytes()),
                crc32(&d[4..]),
                crc32(&d[4..]),
                prefix ^ crc32(text.as_bytes())
            );
            let head: String = text.chars().take(160).collect();
            println!("  text: {head}");
        }
        Err(e) => println!("  text: {e}"),
    }
}

fn main() {
    let arg = std::env::args().nth(1).expect("path");
    let path = Path::new(&arg);
    if arg.ends_with(".vpk") {
        let vpk = VpkDir::open(path).unwrap();
        for p in vpk.entries.keys() {
            if p.ends_with(".vcss_c") {
                dump(p, &vpk.read(p).unwrap());
            }
        }
    } else {
        dump(&arg, &std::fs::read(path).unwrap());
    }
}
