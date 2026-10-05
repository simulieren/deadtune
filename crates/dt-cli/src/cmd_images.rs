//! images export-all: every UI image as PNG and SVG, with a manifest.

use std::ops::ControlFlow;
use std::path::PathBuf;

use dt_core::snapshot::human_bytes;
use dt_core::snapshot::images::{self, ExportOptions, ImageSource};

use crate::args::{Args, CliResult};
use crate::env::Env;

pub fn export_all(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let source = match args.value("from") {
        Some(dir) => ImageSource::folder(&PathBuf::from(dir)),
        None => ImageSource::game(&paths)?,
    };
    let buildid = source.buildid(&paths);
    let options = ExportOptions {
        folder: args.value("folder").map(str::to_string),
        zip: args.switch("zip"),
    };
    let out = match args.value("out") {
        Some(dir) => PathBuf::from(dir),
        None => {
            images::default_folder(&env.data_dir, buildid.as_deref(), options.folder.as_deref())
        }
    };
    let done = images::export_all(&source, &out, buildid, &options, &mut |p| {
        if p.done % 200 == 0 || p.done == p.total {
            eprintln!("{}/{} {}", p.done, p.total, p.path);
        }
        ControlFlow::Continue(())
    })?;
    let m = &done.manifest;
    println!(
        "Exported {} of {} images (build {}) to {}: {} in {:.1}s",
        m.exported,
        m.total,
        m.buildid.as_deref().unwrap_or("unknown"),
        done.folder.display(),
        human_bytes(done.bytes),
        done.elapsed.as_secs_f64()
    );
    println!(
        "Manifest: {}",
        done.folder.join(images::MANIFEST_JSON).display()
    );
    if let Some(zip) = &done.zip {
        println!("Zip: {}", zip.display());
    }
    if m.failed > 0 {
        println!(
            "{} failed (also in {}):",
            m.failed,
            done.folder.join(images::FAILURES).display()
        );
        for image in m.failures() {
            println!(
                "  {}: {}",
                image.path,
                image.error.as_deref().unwrap_or_default()
            );
        }
    }
    Ok(())
}
