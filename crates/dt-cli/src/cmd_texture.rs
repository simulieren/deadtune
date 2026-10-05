//! texture png | svg: one compiled image to a file you can open.

use std::path::Path;

use dt_core::texture::{self, png, svg};

use crate::args::{Args, CliResult, fail};
use crate::env::Env;

fn read(path: &str) -> Result<Vec<u8>, crate::args::CliError> {
    std::fs::read(path).map_err(|e| fail(format!("{}: {e}", Path::new(path).display())))
}

pub fn to_png(_: &Env, args: &Args) -> CliResult {
    let [input, output] = args.positionals("<in.vtex_c> <out.png>")?;
    let image = texture::decode(&read(input)?)?;
    std::fs::write(output, png::write(&image)?)?;
    println!("{output}: {}x{}", image.width, image.height);
    Ok(())
}

pub fn to_svg(_: &Env, args: &Args) -> CliResult {
    let [input, output] = args.positionals("<in.vsvg_c> <out.svg>")?;
    let text = svg::svg_text(&read(input)?)?;
    std::fs::write(output, &text)?;
    println!("{output}: {} bytes of SVG", text.len());
    Ok(())
}
