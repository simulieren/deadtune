//! Rewrites `catalog/catalog.toml` from the research CSV and `catalog/curated.toml`.
//! Run: `cargo run -p dt-core --example gen_catalog`

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let csv = std::fs::read_to_string(root.join("research/data/convar_catalog.csv"))?;
    let curated = std::fs::read_to_string(root.join("catalog/curated.toml"))?;
    let out = root.join("catalog/catalog.toml");
    std::fs::write(&out, dt_core::catalog::generate(&csv, &curated)?)?;
    println!("wrote {}", out.display());
    Ok(())
}
