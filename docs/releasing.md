# Releasing DeadTune

Releases are built on the Mac by `scripts/release-local.sh`. It cross-compiles the Windows build with MinGW and uploads it with `gh`. No GitHub Actions minutes are used.

One-time setup:

```sh
brew install mingw-w64 minisign
rustup target add x86_64-pc-windows-gnu
gh auth login
```

You also need the release signing key (see [Signing key](#signing-key)).

## Cutting a release

```sh
scripts/release-local.sh              # testing: replace the rolling `testing` prerelease with committed HEAD
scripts/release-local.sh minor        # bump 0.Y.0 to 0.(Y+1).0, commit, tag vX, push, publish
scripts/release-local.sh first        # publish the current Cargo.toml version without a bump
scripts/release-local.sh --no-upload  # build and sign everything into target/release-local, upload nothing
```

`minor` and `first` run only on `main`, with a clean tree, in sync with `origin/main`. `testing` needs HEAD pushed to some branch, because the release points at that commit. Every mode builds from `git archive` of HEAD, so uncommitted changes are never included.

The script checks that `minisign` is installed and that the key matches `crates/dt-core/src/update/release.pub` before it bumps, tags or builds anything.

## What gets published

Every release, testing or versioned, carries five files:

| File | Purpose |
|---|---|
| `deadtune-v0.Y.0-windows-x64.zip` (testing: `deadtune-windows-x64.zip`) | The zip people download by hand |
| `<zip>.sha256` | Checksum used by `scripts/get-testing.ps1` |
| `deadtune-windows-x64.exe` | The raw GUI executable, what the in-app updater downloads |
| `deadtune-windows-x64.exe.minisig` | Its minisign signature |
| `latest.json` | The update manifest the app reads |

The build embeds the commit it was built from (`DEADTUNE_COMMIT`). The signature's trusted comment is `deadtune <version> <commit> windows-x64`, the same string as `dt_core::update::trusted_comment`. The app rejects a download whose signature, checksum, size, version or commit does not match the manifest.

`latest.json` is written by `cargo run -p dt-core --example make_manifest`. It holds the channel (`stable` for `minor`/`first`, `testing` for testing), version, commit, publish time, the release page URL and the exe's download URL, size, SHA-256 and signature.

Where the app looks:

- Stable: `https://github.com/simulieren/deadtune/releases/latest/download/latest.json`. GitHub resolves `latest` to the newest release that is not a prerelease, so `minor` and `first` are published with `--latest`.
- Testing: `https://github.com/simulieren/deadtune/releases/download/testing/latest.json`. The `testing` release stays a prerelease, so it never becomes `latest`.

Both URLs only work for the app if the repository is public. A private repo needs a GitHub login to download release assets.

## Signing key

The private key is at `~/.config/deadtune/release.key` on the release Mac. It is unencrypted, so signing never prompts. Point `DEADTUNE_RELEASE_KEY` at another path to use a key stored elsewhere. The public half is committed as `crates/dt-core/src/update/release.pub` and compiled into the app.

Back the key up now, somewhere offline and outside this Mac (a password manager entry or an encrypted USB stick). If it is lost, no new release can be signed with a key that installed apps trust. Every user would have to download and install a new build by hand once, and that build would carry a new public key.

Never commit the private key or paste it into a chat or issue.

## Rotating the key

Installed apps trust only the public key they were built with. A switch needs one transition release that is signed with the old key and contains the new public key.

1. Generate the new pair outside the default path:
   ```sh
   minisign -G -W -p new-release.pub -s ~/.config/deadtune/release.key.new
   ```
2. Copy the old public key aside (`cp crates/dt-core/src/update/release.pub old-release.pub`), replace `release.pub` with `new-release.pub`, and commit.
3. Cut the transition release with the old key:
   ```sh
   DEADTUNE_RELEASE_KEY=~/.config/deadtune/release.key DEADTUNE_RELEASE_PUB=old-release.pub scripts/release-local.sh minor
   ```
   Do the same for `testing` if people run testing builds.
4. Move the new key into place: `mv ~/.config/deadtune/release.key.new ~/.config/deadtune/release.key`. Back it up.
5. Later releases need no overrides.

Users who skip the transition release and jump straight to a later one cannot verify it. Leave the transition release as the newest stable release for a while before publishing the next one. If the old key leaked, skip the transition and tell users to reinstall by hand.

## Testing the updater locally

```sh
scripts/release-local.sh --no-upload
```

This writes the zip, `deadtune-windows-x64.exe`, its `.minisig` and `latest.json` (channel `testing`) to `target/release-local`, exactly as a testing upload would. Then check them:

```sh
cd target/release-local
minisign -Vm deadtune-windows-x64.exe -p ../../crates/dt-core/src/update/release.pub
python3 -m json.tool latest.json
```

The first command prints the trusted comment, which must read `deadtune <version> <commit> windows-x64`. To run the app's own check, call `dt_core::update::verify` on the exe with the manifest's `windows-x64` asset, version and commit, `Target::WindowsX64` and `RELEASE_PUBKEY`. It returns `Ok(())` for a good build.

The download itself can only be tested against a real release. Publish a testing build, install an older one on Windows, and see that the app offers the newer one. See "Updates" in `docs/testing-windows.md`.
