//! snapshot take | list | diff

use std::ops::ControlFlow;

use dt_core::snapshot::{self, Category, Selection, diff, human_bytes, store};

use crate::args::{Args, CliResult, usage};
use crate::env::Env;

pub fn take(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let paths = env.paths()?;
    let mut selection = Selection::default();
    if let Some(list) = args.value("categories") {
        selection.categories = list
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                Category::parse(s).ok_or_else(|| {
                    let keys: Vec<&str> = Category::ALL.iter().map(|c| c.key()).collect();
                    usage(format!(
                        "unknown category {s:?}; expected one of {}",
                        keys.join(", ")
                    ))
                })
            })
            .collect::<Result<_, _>>()?;
    }
    selection.decode = !args.switch("no-decode");
    if let Some(cap) = args.value("size-cap") {
        selection.size_cap = match cap {
            "none" => None,
            mb => Some(
                mb.parse::<u64>().map_err(|_| {
                    usage(format!(
                        "--size-cap takes a number of MB or none, not {mb:?}"
                    ))
                })? << 20,
            ),
        };
    }
    let out = snapshot::take(&paths, &env.data_dir, &selection, &mut |p| {
        if p.done % 100 == 0 || p.done == p.total {
            eprintln!("{}/{} {}", p.done, p.total, p.path);
        }
        ControlFlow::Continue(())
    })?;
    println!(
        "Snapshot {} (build {}): {} files, {} written, {} already there, {} in {:.1}s",
        out.folder.display(),
        out.manifest.buildid.as_deref().unwrap_or("unknown"),
        out.manifest.files.len(),
        out.written,
        out.reused,
        human_bytes(out.bytes),
        out.elapsed.as_secs_f64()
    );
    Ok(())
}

pub fn list(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let all = store::list(&store::dir(&env.data_dir))?;
    if all.is_empty() {
        println!("No snapshots yet. Run `deadtune-cli snapshot take`.");
        return Ok(());
    }
    for s in all {
        let state = if s.complete { "" } else { "  (incomplete)" };
        println!(
            "{}  build {}  {}  {} files  {}{state}",
            s.name,
            s.buildid.as_deref().unwrap_or("unknown"),
            s.taken
                .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_default(),
            s.files,
            human_bytes(s.bytes)
        );
        for report in &s.reports {
            println!("    report: {report}");
        }
    }
    Ok(())
}

pub fn diff(env: &Env, args: &Args) -> CliResult {
    let [old, new] =
        args.positionals::<2>("two snapshots (names, build ids, latest or previous)")?;
    let dir = store::dir(&env.data_dir);
    let (old, new) = (store::find(&dir, old)?, store::find(&dir, new)?);
    let report = snapshot::compare(&old.folder, &new.folder)?;
    let path = diff::write(&report)?;
    if args.switch("full") {
        print!("{}", report.markdown());
    } else {
        print!("{}", report.summary());
    }
    println!("Report: {}", path.display());
    Ok(())
}
