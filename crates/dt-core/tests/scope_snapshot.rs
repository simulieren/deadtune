//! The Vindicta scope downscale against the real game's scope texture header, from a
//! game file snapshot (`DEADTUNE_GAME_SNAPSHOT=<snapshot folder>`). Valve's files never
//! enter the repo, so without the variable these tests print a note and pass. The pixels
//! are synthetic (Tamara's 1080 px vignette upscaled), the header is the game's own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use dt_core::addons::guard::{Event, Guard, Observation, installed_paks};
use dt_core::addons::install::{execute, installed_state, plan, remove_now};
use dt_core::addons::native_scope::{TEXTURE, target_dims};
use dt_core::addons::verify::verify_installed;
use dt_core::addons::{Action, AddonId, AddonsConfig, InstalledState, ScopeOptions};
use dt_core::hud::install::{GAME_PAK, addons_dir};
use dt_core::hud::resource::Resource;
use dt_core::hud::vpk::{VpkDir, VpkWriter};
use dt_core::locate::{GamePaths, from_game_root};
use dt_core::snapshot::Manifest;
use dt_core::texture::resample::Image;
use dt_core::texture::vtex::{Flags, Vtex};

const VANILLA: &str =
    include_str!("../../../research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi");
const UPSTREAM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../research/configs/OptimizationLock/Various Addons Relating to Performance/",
    "Vindicta Scope Downscale/pak89_dir.vpk"
);
const FALLBACK_BITS: u32 = 1;
const PIXEL_BYTES: usize = 2048 * 2048 * 4;

fn snapshot() -> Option<PathBuf> {
    let dir = std::env::var_os("DEADTUNE_GAME_SNAPSHOT").map(PathBuf::from);
    if dir.is_none() {
        eprintln!("DEADTUNE_GAME_SNAPSHOT is not set; skipping the real scope texture checks");
    }
    dir
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// `(kind, payload)` of every extra-data entry of a VTEX DATA block.
fn extras(data: &[u8]) -> Vec<(u32, &[u8])> {
    let table = 32 + u32_at(data, 32) as usize;
    (0..u32_at(data, 36) as usize)
        .map(|i| {
            let at = table + 12 * i;
            let payload = at + 4 + u32_at(data, at + 4) as usize;
            (
                u32_at(data, at),
                &data[payload..payload + u32_at(data, at + 8) as usize],
            )
        })
        .collect()
}

fn data_block(res: &Resource) -> &[u8] {
    &res.block(b"DATA").expect("DATA block").data
}

fn block_layout(res: &Resource) -> Vec<[u8; 4]> {
    res.blocks.iter().map(|b| b.name).collect()
}

fn upstream() -> Vec<u8> {
    VpkDir::open(Path::new(UPSTREAM))
        .unwrap()
        .read(TEXTURE)
        .unwrap()
}

/// The real header with the game's declared size checked against the manifest.
fn real_header(snapshot: &Path) -> (Vec<u8>, u64) {
    let manifest = Manifest::load(snapshot).unwrap();
    let entry = manifest
        .entry(TEXTURE)
        .expect("scope texture in the manifest");
    let header = std::fs::read(snapshot.join("raw").join(format!("{TEXTURE}.header")))
        .unwrap_or_else(|_| {
            let full = std::fs::read(snapshot.join("raw").join(TEXTURE)).unwrap();
            full[..Vtex::parse(&full).unwrap().pixel_start()].to_vec()
        });
    (header, entry.size)
}

/// Tamara's 1080 px pixels upscaled bilinearly to 2048 px: the same alpha vignette at
/// the game's resolution.
fn upscaled_pixels(up: &[u8]) -> Vec<u8> {
    let v = Vtex::parse(up).unwrap();
    let (src, n) = (&up[v.pixel_start()..], usize::from(v.width));
    let at = |x: usize, y: usize, k: usize| f32::from(src[(y * n + x) * 4 + k]);
    let scale = n as f32 / 2048.0;
    let mut out = Vec::with_capacity(PIXEL_BYTES);
    for y in 0..2048 {
        let fy = ((y as f32 + 0.5) * scale - 0.5).clamp(0.0, (n - 1) as f32);
        let (y0, ty) = (fy as usize, fy.fract());
        let y1 = (y0 + 1).min(n - 1);
        for x in 0..2048 {
            let fx = ((x as f32 + 0.5) * scale - 0.5).clamp(0.0, (n - 1) as f32);
            let (x0, tx) = (fx as usize, fx.fract());
            let x1 = (x0 + 1).min(n - 1);
            for k in 0..4 {
                let top = at(x0, y0, k) * (1.0 - tx) + at(x1, y0, k) * tx;
                let bottom = at(x0, y1, k) * (1.0 - tx) + at(x1, y1, k) * tx;
                out.push((top * (1.0 - ty) + bottom * ty + 0.5) as u8);
            }
        }
    }
    out
}

fn psnr(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mse = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| (f64::from(x) - f64::from(y)).powi(2))
        .sum::<f64>()
        / a.len() as f64;
    10.0 * (255.0 * 255.0 / mse).log10()
}

/// A Steam library whose split pak01 stores the scope texture in `pak01_001.vpk`, as
/// Valve's archives keep file data in the numbered chunks, never in the dir file.
fn fake_install(original: &[u8]) -> (tempfile::TempDir, GamePaths) {
    let tmp = tempfile::tempdir().unwrap();
    let steamapps = tmp.path().join("steamapps");
    let root = steamapps.join("common/Deadlock");
    let citadel = root.join("game/citadel");
    std::fs::create_dir_all(citadel.join("cfg")).unwrap();
    std::fs::write(citadel.join("gameinfo.gi"), VANILLA).unwrap();
    std::fs::write(
        steamapps.join("appmanifest_1422450.acf"),
        "\"AppState\"\n{\n\t\"appid\"\t\t\"1422450\"\n\t\"buildid\"\t\t\"25712201\"\n}\n",
    )
    .unwrap();
    let mut pak = VpkWriter::create(&citadel.join(GAME_PAK), 1 << 20).unwrap();
    pak.add("models/filler.vmdl_c", &vec![7u8; 1 << 20])
        .unwrap();
    pak.add(TEXTURE, original).unwrap();
    pak.finish().unwrap();
    (tmp, from_game_root(&root).unwrap())
}

#[test]
fn the_real_header_is_what_the_builder_expects() {
    let Some(snapshot) = snapshot() else { return };
    let (header, size) = real_header(&snapshot);
    assert_eq!(header.len(), 2100);
    assert_eq!(size, 16_779_316);

    let res = Resource::parse(&header).unwrap();
    assert_eq!(res.to_bytes(), header, "the header writer round-trips it");
    assert_eq!(block_layout(&res), [*b"RED2", *b"DATA"]);
    let data = data_block(&res);
    assert_eq!(data.len(), 1076);
    let extra: Vec<(u32, usize)> = extras(data).iter().map(|(k, p)| (*k, p.len())).collect();
    assert_eq!(extra, [(FALLBACK_BITS, 1024)]);

    let v = Vtex::parse(&header).unwrap();
    assert_eq!((v.width, v.height, v.depth), (2048, 2048, 1));
    assert_eq!(v.format.name(), "BGRA8888");
    assert_eq!(v.flags, Flags::NO_LOD);
    assert_eq!(v.mips.len(), 1);
    assert_eq!(v.display_rect, None);
    assert_eq!(v.pixel_start(), 2100);
    assert_eq!(v.pixel_len(), PIXEL_BYTES);
    assert_eq!((v.pixel_start() + v.pixel_len()) as u64, size);
}

#[test]
fn the_real_original_builds_installs_verifies_and_removes_at_every_size() {
    let Some(snapshot) = snapshot() else { return };
    let (header, size) = real_header(&snapshot);
    let up = upstream();
    let original = [header.clone(), upscaled_pixels(&up)].concat();
    assert_eq!(original.len() as u64, size);

    let (tmp, paths) = fake_install(&original);
    let game = VpkDir::open(&paths.citadel_dir.join(GAME_PAK)).unwrap();
    assert_eq!(game.entries[TEXTURE].archive_index, 1);
    assert_eq!(game.read(TEXTURE).unwrap(), original);

    let state = tmp.path().join("data");
    let mut guard = Guard::default();
    let up_v = Vtex::parse(&up).unwrap();
    let up_res = Resource::parse(&up).unwrap();
    let game_res = Resource::parse(&header).unwrap();
    for side in [720u16, 1080, 1440] {
        let config = AddonsConfig {
            enabled: [AddonId::VindictaScope].into_iter().collect(),
            scope: ScopeOptions { side },
            ..AddonsConfig::default()
        };
        let planned = plan(&paths, &config, &state).unwrap();
        let entry = planned.get(AddonId::VindictaScope).unwrap();
        assert!(matches!(entry.action, Action::Write(_)), "{side}");
        execute(&planned, &paths, &state).unwrap();
        assert!(plan(&paths, &config, &state).unwrap().is_empty(), "{side}");
        assert!(matches!(
            installed_state(&paths, &state).unwrap()[&AddonId::VindictaScope],
            InstalledState::Current(_)
        ));

        let reports = verify_installed(&paths, &state);
        assert_eq!(reports.len(), 1);
        let verified = reports[0].result.as_ref().unwrap();
        assert!(verified.problems.is_empty(), "{side}: {}", reports[0]);
        assert_eq!(verified.entries, 1);

        let pak = VpkDir::open(&entry.path).unwrap();
        assert_eq!(pak.entries.len(), 1);
        let out = pak.read(TEXTURE).unwrap();
        let v = Vtex::parse(&out).unwrap();
        let (w, h) = target_dims(2048, 2048, side);
        assert_eq!((v.width, v.height), (side, side));
        assert_eq!((v.width, v.height), (w, h));
        assert_eq!(
            (v.depth, v.format, v.flags, v.mips.len(), v.display_rect),
            (up_v.depth, up_v.format, up_v.flags, 1, up_v.display_rect),
            "{side}: every dimension-independent field as Tamara's"
        );
        assert_eq!(v.pixel_start() + v.pixel_len(), out.len());
        assert_eq!(out.len(), 2100 + usize::from(side) * usize::from(side) * 4);

        let res = Resource::parse(&out[..v.pixel_start()]).unwrap();
        assert_eq!(block_layout(&res), block_layout(&up_res));
        assert_eq!(
            res.block(b"RED2"),
            game_res.block(b"RED2"),
            "the game's editor info"
        );
        let (ours, theirs, games) = (data_block(&res), data_block(&up_res), data_block(&game_res));
        assert_eq!(ours.len(), theirs.len());
        assert_eq!(
            ours[20..24],
            [side.to_le_bytes(), side.to_le_bytes()].concat()
        );
        assert_eq!(
            [&ours[..4], &ours[24..52]],
            [&theirs[..4], &theirs[24..52]],
            "{side}: version, flags, depth, format, mips, picmip and extra table as Tamara's"
        );
        assert_eq!(
            [&ours[4..20], &ours[52..]],
            [&games[4..20], &games[52..]],
            "{side}: reflectivity and fallback thumbnail stay the game's"
        );

        let px = &out[v.pixel_start()..];
        let reference = Image::new(1080, 1080, 4, up[up_v.pixel_start()..].to_vec())
            .unwrap()
            .resample(u32::from(side), u32::from(side));
        let db = psnr(px, &reference.data);
        eprintln!("{side}: {db:.1} dB against Tamara's art scaled straight to {side}");
        assert!(db > 40.0, "{side}: {db:.1} dB");
        if side == 1080 {
            let db = psnr(px, &up[up_v.pixel_start()..]);
            eprintln!("1080: {db:.1} dB against Tamara's file");
            assert!(db > 40.0, "{db:.1} dB against Tamara's file");
        }

        let installed = installed_paks(&paths, &state);
        assert_eq!(installed.len(), 1);
        let launched = installed[0].written_at.unwrap() + Duration::from_secs(5);
        let event = guard.step(&Observation {
            running: true,
            started_at: Some(launched),
            now: launched,
            installed: &installed,
            lines: &[],
        });
        assert_eq!(
            event,
            Some(Event::Started {
                changed: vec![AddonId::VindictaScope]
            }),
            "{side}: the launch guard puts the new pak on trial"
        );
        let event = guard.step(&Observation {
            running: true,
            started_at: Some(launched),
            now: launched,
            installed: &installed,
            lines: &["DEADTUNE_BOOT 0.9.0".into()],
        });
        assert_eq!(
            event,
            Some(Event::Passed {
                ids: vec![AddonId::VindictaScope]
            })
        );
        assert!(guard.is_verified(AddonId::VindictaScope, &installed[0].sha256));
        guard.step(&Observation {
            running: false,
            started_at: None,
            now: SystemTime::now(),
            installed: &installed,
            lines: &[],
        });
    }

    let at_full = AddonsConfig {
        enabled: [AddonId::VindictaScope].into_iter().collect(),
        scope: ScopeOptions { side: 2048 },
        ..AddonsConfig::default()
    };
    let planned = plan(&paths, &at_full, &state).unwrap();
    assert_eq!(
        planned.get(AddonId::VindictaScope).unwrap().action,
        Action::Remove,
        "2048 px is the game's own size: nothing to install, the 1440 pak goes"
    );
    execute(&planned, &paths, &state).unwrap();
    assert!(installed_paks(&paths, &state).is_empty());
    assert!(plan(&paths, &at_full, &state).unwrap().is_empty());

    let config = AddonsConfig {
        enabled: [AddonId::VindictaScope].into_iter().collect(),
        ..AddonsConfig::default()
    };
    execute(&plan(&paths, &config, &state).unwrap(), &paths, &state).unwrap();
    assert_eq!(installed_paks(&paths, &state).len(), 1);
    assert!(remove_now(AddonId::VindictaScope, &paths, &state).unwrap());
    assert!(installed_paks(&paths, &state).is_empty());
    let left: BTreeMap<_, _> = installed_state(&paths, &state).unwrap();
    assert!(left.is_empty());
    assert!(
        std::fs::read_dir(addons_dir(&paths)).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("pak74")),
        "Remove from game deletes the pak"
    );
    assert_eq!(game.read(TEXTURE).unwrap(), original, "pak01 untouched");
}
