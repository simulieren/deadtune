//! bench import | list | compare

use std::path::Path;

use dt_core::bench::{self, BenchRecord, Metrics};

use crate::args::{Args, CliResult, fail};
use crate::env::{self, Env};

fn print_metrics(m: &Metrics) {
    println!(
        "  {} frames over {:.1}s  avg {:.1} fps  1% low {:.1}  0.1% low {:.1}",
        m.frames, m.duration_s, m.avg_fps, m.low_1_fps, m.low_01_fps
    );
}

pub fn import(env: &Env, args: &Args) -> CliResult {
    let [csv] = args.positionals("<csv>")?;
    let capture = bench::parse_capture(&env::read(Path::new(csv))?)?;
    let record = BenchRecord {
        profile: args.required("profile")?.to_string(),
        label: args.required("label")?.to_string(),
        recorded: std::time::SystemTime::now().into(),
        metrics: bench::metrics(&capture)?,
        screenshots: Vec::new(),
    };
    bench::save_record(&env.bench_dir(), &record)?;
    println!(
        "Imported {:?} capture as {}/{}",
        capture.tool, record.profile, record.label
    );
    print_metrics(&record.metrics);
    Ok(())
}

pub fn list(env: &Env, args: &Args) -> CliResult {
    args.positionals::<0>("no positional arguments")?;
    let profile = args.required("profile")?;
    let records = bench::load_records(&env.bench_dir(), profile)?;
    if records.is_empty() {
        println!("No bench runs for {profile:?}.");
    }
    for r in records {
        println!("{}  {}", r.recorded.format("%Y-%m-%d %H:%M:%S"), r.label);
        print_metrics(&r.metrics);
    }
    Ok(())
}

pub fn compare(env: &Env, args: &Args) -> CliResult {
    let [profile, a, b] = args.positionals("<profile> <labelA> <labelB>")?;
    let records = bench::load_records(&env.bench_dir(), profile)?;
    let newest = |label: &str| {
        records
            .iter()
            .find(|r| r.label == label)
            .ok_or_else(|| fail(format!("no run labelled {label:?} for {profile:?}")))
    };
    let (ra, rb) = (newest(a)?, newest(b)?);
    let delta = bench::compare(&ra.metrics, &rb.metrics);
    println!("{a}:");
    print_metrics(&ra.metrics);
    println!("{b}:");
    print_metrics(&rb.metrics);
    println!(
        "{b} vs {a}: avg {:+.1}%  1% low {:+.1}%  0.1% low {:+.1}%",
        delta.avg_fps_pct, delta.low_1_pct, delta.low_01_pct
    );
    Ok(())
}
