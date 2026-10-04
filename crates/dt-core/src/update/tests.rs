use std::collections::BTreeMap;
use std::fs;

use super::*;

const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/update/asset.bin");
const FIXTURE_SIG: &str = include_str!("../../tests/fixtures/update/asset.bin.minisig");
const FIXTURE_SIG_V998: &str = include_str!("../../tests/fixtures/update/asset.bin.v998.minisig");
const FIXTURE_SIG_OTHER_KEY: &str =
    include_str!("../../tests/fixtures/update/asset.bin.otherkey.minisig");
const TEST_PUBKEY: &str = include_str!("../../tests/fixtures/update/test.pub");
const V: Version = Version(9, 9, 9);
const COMMIT: &str = "abc1234";

fn fixture_asset(minisig: &str) -> Asset {
    Asset {
        url: "https://example.invalid/deadtune.exe".into(),
        size: FIXTURE.len() as u64,
        sha256: crate::backup::sha256_hex(FIXTURE),
        minisig: minisig.into(),
    }
}

fn verify_fixture(bytes: &[u8], asset: &Asset) -> Result<(), UpdateError> {
    verify(bytes, asset, V, COMMIT, Target::WindowsX64, TEST_PUBKEY)
}

fn manifest(channel: Channel, version: Version, commit: &str) -> Manifest {
    Manifest {
        channel,
        version,
        commit: commit.into(),
        published: "2026-10-05T00:00:00Z".into(),
        notes_url: "https://github.com/simulieren/deadtune/releases".into(),
        assets: BTreeMap::from([(Target::WindowsX64, fixture_asset(FIXTURE_SIG))]),
    }
}

fn current(version: Version, commit: Option<&str>) -> Current {
    Current {
        version,
        commit: commit.map(str::to_owned),
        target: Target::WindowsX64,
    }
}

fn is_available(offer: &Offer) -> bool {
    matches!(offer, Offer::Available { .. })
}

#[test]
fn version_parses_strictly_and_round_trips() {
    assert_eq!("0.3.0".parse::<Version>().unwrap(), Version(0, 3, 0));
    assert_eq!(Version(10, 0, 12).to_string(), "10.0.12");
    for bad in [
        "",
        "1",
        "1.2",
        "1.2.3.4",
        "v1.2.3",
        "1.2.3-rc1",
        "+1.2.3",
        "1..3",
        "1.2. 3",
    ] {
        assert!(bad.parse::<Version>().is_err(), "{bad:?} parsed");
    }
    assert!(Version(0, 10, 0) > Version(0, 9, 9));
}

#[test]
fn trusted_comment_names_version_commit_and_kebab_target() {
    assert_eq!(
        trusted_comment(V, COMMIT, Target::WindowsX64),
        "deadtune 9.9.9 abc1234 windows-x64"
    );
    assert_eq!(
        trusted_comment(Version(0, 3, 0), "deadbee", Target::LinuxX64),
        "deadtune 0.3.0 deadbee linux-x64"
    );
}

#[test]
fn this_build_reflects_the_package() {
    let Some(me) = Current::this_build() else {
        assert_eq!(Target::CURRENT, None);
        return;
    };
    assert_eq!(me.version.to_string(), env!("CARGO_PKG_VERSION"));
    assert_eq!(me.commit.as_deref(), option_env!("DEADTUNE_COMMIT"));
}

#[test]
fn decide_stable_offers_only_strictly_newer() {
    let me = current(Version(0, 3, 0), Some("aaaaaaa"));
    let at = |v| decide(&me, &manifest(Channel::Stable, v, "bbbbbbb"), None);
    assert!(is_available(&at(Version(0, 4, 0))));
    assert_eq!(at(Version(0, 3, 0)), Offer::UpToDate);
    assert_eq!(at(Version(0, 2, 0)), Offer::UpToDate);
}

#[test]
fn decide_available_carries_the_manifest_fields() {
    let me = current(Version(0, 3, 0), None);
    let m = manifest(Channel::Stable, Version(0, 4, 0), "bbbbbbb");
    assert_eq!(
        decide(&me, &m, None),
        Offer::Available {
            version: Version(0, 4, 0),
            commit: "bbbbbbb".into(),
            notes_url: m.notes_url.clone(),
            asset: m.assets[&Target::WindowsX64].clone(),
        }
    );
}

#[test]
fn decide_testing_offers_other_commits_never_downgrades() {
    let me = current(Version(0, 3, 0), Some("aaaaaaa"));
    let at = |v, c| decide(&me, &manifest(Channel::Testing, v, c), None);
    assert_eq!(at(Version(0, 3, 0), "aaaaaaa"), Offer::UpToDate);
    assert!(is_available(&at(Version(0, 3, 0), "bbbbbbb")));
    assert!(is_available(&at(Version(0, 4, 0), "aaaaaaa")));
    assert_eq!(at(Version(0, 2, 0), "bbbbbbb"), Offer::UpToDate);
}

#[test]
fn decide_testing_dev_build_needs_a_strictly_newer_version() {
    let dev = current(Version(0, 3, 0), None);
    let at = |v| decide(&dev, &manifest(Channel::Testing, v, "bbbbbbb"), None);
    assert_eq!(at(Version(0, 3, 0)), Offer::UpToDate);
    assert!(is_available(&at(Version(0, 4, 0))));
}

#[test]
fn decide_skipped_only_for_that_exact_offered_version() {
    let me = current(Version(0, 3, 0), Some("aaaaaaa"));
    let m = manifest(Channel::Stable, Version(0, 4, 0), "bbbbbbb");
    assert_eq!(decide(&me, &m, Some(Version(0, 4, 0))), Offer::Skipped);
    assert!(is_available(&decide(&me, &m, Some(Version(0, 3, 5)))));
    let older = manifest(Channel::Stable, Version(0, 2, 0), "bbbbbbb");
    assert_eq!(decide(&me, &older, Some(Version(0, 2, 0))), Offer::UpToDate);
}

#[test]
fn decide_without_an_asset_for_this_target_is_up_to_date() {
    let me = Current {
        target: Target::LinuxX64,
        ..current(Version(0, 3, 0), Some("aaaaaaa"))
    };
    let m = manifest(Channel::Stable, Version(0, 4, 0), "bbbbbbb");
    assert_eq!(decide(&me, &m, None), Offer::UpToDate);
}

#[test]
fn release_pubkey_decodes() {
    minisign_verify::PublicKey::decode(RELEASE_PUBKEY).expect("embedded release key");
}

#[test]
fn verify_accepts_the_signed_fixture() {
    verify_fixture(FIXTURE, &fixture_asset(FIXTURE_SIG)).unwrap();
}

#[test]
fn verify_rejects_wrong_size() {
    let asset = Asset {
        size: FIXTURE.len() as u64 + 1,
        ..fixture_asset(FIXTURE_SIG)
    };
    assert!(matches!(
        verify_fixture(FIXTURE, &asset),
        Err(UpdateError::Size { .. })
    ));
}

#[test]
fn verify_rejects_wrong_sha() {
    let asset = Asset {
        sha256: crate::backup::sha256_hex(b"something else"),
        ..fixture_asset(FIXTURE_SIG)
    };
    assert!(matches!(
        verify_fixture(FIXTURE, &asset),
        Err(UpdateError::Checksum)
    ));
}

/// The manifest's sha256 is unsigned: an attacker who controls it still fails on
/// the signature.
#[test]
fn verify_rejects_a_tampered_byte_even_with_a_matching_sha() {
    let mut tampered = FIXTURE.to_vec();
    tampered[0] ^= 1;
    let asset = Asset {
        sha256: crate::backup::sha256_hex(&tampered),
        ..fixture_asset(FIXTURE_SIG)
    };
    assert!(matches!(
        verify_fixture(&tampered, &asset),
        Err(UpdateError::Signature(_))
    ));
}

#[test]
fn verify_rejects_a_valid_signature_for_another_release() {
    let err = verify_fixture(FIXTURE, &fixture_asset(FIXTURE_SIG_V998)).unwrap_err();
    let UpdateError::WrongRelease { got, expected } = err else {
        panic!("expected WrongRelease, got {err:?}");
    };
    assert_eq!(got, "deadtune 9.9.8 abc1234 windows-x64");
    assert_eq!(expected, "deadtune 9.9.9 abc1234 windows-x64");
    let wrong_target = verify(
        FIXTURE,
        &fixture_asset(FIXTURE_SIG),
        V,
        COMMIT,
        Target::LinuxX64,
        TEST_PUBKEY,
    );
    assert!(matches!(
        wrong_target,
        Err(UpdateError::WrongRelease { .. })
    ));
}

#[test]
fn verify_rejects_a_signature_by_another_key() {
    assert!(matches!(
        verify_fixture(FIXTURE, &fixture_asset(FIXTURE_SIG_OTHER_KEY)),
        Err(UpdateError::Signature(_))
    ));
    assert!(matches!(
        verify(
            FIXTURE,
            &fixture_asset(FIXTURE_SIG),
            V,
            COMMIT,
            Target::WindowsX64,
            RELEASE_PUBKEY,
        ),
        Err(UpdateError::Signature(_))
    ));
}

fn exe_in(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let exe = dir.path().join("deadtune.exe");
    fs::write(&exe, b"old build").unwrap();
    exe
}

#[test]
fn install_swaps_the_exe_and_keeps_the_old_one_aside() {
    let dir = tempfile::tempdir().unwrap();
    let exe = exe_in(&dir);
    let old = install(&exe, b"new build").unwrap();
    assert_eq!(old, dir.path().join("deadtune.exe.old"));
    assert_eq!(fs::read(&exe).unwrap(), b"new build");
    assert_eq!(fs::read(&old).unwrap(), b"old build");
    assert!(!dir.path().join("deadtune.exe.new").exists());
}

#[test]
fn install_overwrites_a_stale_new_and_old_from_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let exe = exe_in(&dir);
    fs::write(dir.path().join("deadtune.exe.new"), b"half writ").unwrap();
    fs::write(dir.path().join("deadtune.exe.old"), b"older build").unwrap();
    let old = install(&exe, b"new build").unwrap();
    assert_eq!(fs::read(&exe).unwrap(), b"new build");
    assert_eq!(fs::read(&old).unwrap(), b"old build");
}

#[test]
fn install_finishes_after_a_crash_between_the_renames() {
    let dir = tempfile::tempdir().unwrap();
    let exe = exe_in(&dir);
    fs::rename(&exe, dir.path().join("deadtune.exe.old")).unwrap();
    fs::write(dir.path().join("deadtune.exe.new"), b"half writ").unwrap();
    let old = install(&exe, b"new build").unwrap();
    assert_eq!(fs::read(&exe).unwrap(), b"new build");
    assert_eq!(fs::read(&old).unwrap(), b"old build");
    install(&exe, b"new build").unwrap();
    assert_eq!(fs::read(&exe).unwrap(), b"new build");
}

#[cfg(unix)]
#[test]
fn install_keeps_the_executable_bit() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let exe = exe_in(&dir);
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o751)).unwrap();
    install(&exe, b"new build").unwrap();
    assert_eq!(
        fs::metadata(&exe).unwrap().permissions().mode() & 0o777,
        0o751
    );
}

#[test]
fn cleanup_removes_leftovers_and_tolerates_none() {
    let dir = tempfile::tempdir().unwrap();
    let exe = exe_in(&dir);
    install(&exe, b"new build").unwrap();
    fs::write(dir.path().join("deadtune.exe.new"), b"stray").unwrap();
    cleanup(&exe);
    cleanup(&exe);
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["deadtune.exe"]);
}

#[test]
fn manifest_json_uses_the_documented_field_names() {
    let m = manifest(Channel::Testing, Version(0, 3, 0), "abc1234");
    let json: serde_json::Value = serde_json::to_value(&m).unwrap();
    let expected = serde_json::json!({
        "channel": "testing",
        "version": "0.3.0",
        "commit": "abc1234",
        "published": "2026-10-05T00:00:00Z",
        "notes_url": "https://github.com/simulieren/deadtune/releases",
        "assets": {
            "windows-x64": {
                "url": "https://example.invalid/deadtune.exe",
                "size": FIXTURE.len(),
                "sha256": crate::backup::sha256_hex(FIXTURE),
                "minisig": FIXTURE_SIG,
            }
        }
    });
    assert_eq!(json, expected);
    let back: Manifest = serde_json::from_value(json).unwrap();
    assert_eq!(back, m);
    let bad = serde_json::to_string(&m)
        .unwrap()
        .replace("\"0.3.0\"", "\"0.3\"");
    assert!(serde_json::from_str::<Manifest>(&bad).is_err());
}
