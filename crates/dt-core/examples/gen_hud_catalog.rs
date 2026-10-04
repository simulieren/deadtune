//! Rewrites `catalog/hud.toml` from the ConVar dumps and `catalog/hud_curated.toml`.
//! Run: `cargo run -p dt-core --example gen_hud_catalog`

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dumps = root.join("research/configs/OptimizationLock");
    let cvarlist = std::fs::read_to_string(dumps.join("cvarlist.txt"))?;
    let convars = std::fs::read_to_string(dumps.join("convars.txt"))?;
    let curated = std::fs::read_to_string(root.join("catalog/hud_curated.toml"))?;
    let out = root.join("catalog/hud.toml");
    std::fs::write(
        &out,
        dt_core::hud::convars::generate(&cvarlist, &convars, &curated)?,
    )?;
    println!("wrote {}", out.display());
    Ok(())
}
