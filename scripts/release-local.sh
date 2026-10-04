#!/usr/bin/env bash
# Cross-compiles the Windows zip on this machine (MinGW, no Actions minutes) and publishes it,
# together with the signed self-update payload (raw exe, .minisig, latest.json). See docs/releasing.md.
#   scripts/release-local.sh              rolling `testing` prerelease from committed HEAD
#   scripts/release-local.sh --no-upload  build and sign everything in target/release-local only
#   scripts/release-local.sh minor        semver release: bump 0.Y.0 -> 0.(Y+1).0, commit, tag, publish
#   scripts/release-local.sh first        publish the current version as its first release (no bump)
# Versioning is minor-only for now: every release is 0.Y.0 (enforced by dt-core's version test).
# Needs: brew install mingw-w64 minisign; rustup target add x86_64-pc-windows-gnu; gh auth login;
# the release key at ~/.config/deadtune/release.key (or $DEADTUNE_RELEASE_KEY), matching
# crates/dt-core/src/update/release.pub (or $DEADTUNE_RELEASE_PUB, only while rotating the key).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
target=x86_64-pc-windows-gnu
mode="${1:-testing}"
case "$mode" in testing|--no-upload|minor|first) ;; *)
  echo "usage: $0 [--no-upload|minor|first]  (only minor releases are allowed for now)" >&2; exit 2 ;;
esac

key="${DEADTUNE_RELEASE_KEY:-$HOME/.config/deadtune/release.key}"
pubkey="${DEADTUNE_RELEASE_PUB:-$root/crates/dt-core/src/update/release.pub}"
command -v minisign >/dev/null || { echo "minisign not found: brew install minisign" >&2; exit 1; }
[ -f "$key" ] || { echo "release key $key not found (set DEADTUNE_RELEASE_KEY; see docs/releasing.md)" >&2; exit 1; }
probe="$(mktemp -d)" && trap 'rm -rf "$probe"' EXIT
echo probe > "$probe/f" && minisign -S -s "$key" -m "$probe/f" </dev/null >/dev/null \
  && minisign -Vqm "$probe/f" -p "$pubkey" \
  || { echo "$key does not match $pubkey (see docs/releasing.md)" >&2; exit 1; }

version() { sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1; }

if [ "$mode" = minor ] || [ "$mode" = first ]; then
  [ "$(git rev-parse --abbrev-ref HEAD)" = main ] || { echo "releases are cut from main" >&2; exit 1; }
  [ -z "$(git status --porcelain --untracked-files=no)" ] || { echo "commit your changes first" >&2; exit 1; }
  git fetch -q origin main
  [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || { echo "main is not in sync with origin/main" >&2; exit 1; }
  current=$(version)
  [[ "$current" =~ ^0\.([0-9]+)\.0$ ]] || { echo "version $current is not 0.Y.0" >&2; exit 1; }
  if [ "$mode" = minor ]; then
    next="0.$((BASH_REMATCH[1] + 1)).0"
    sed -i '' "s/^version = \"$current\"/version = \"$next\"/" Cargo.toml
    cargo update -q -w
    cargo test -q -p dt-core --lib version_is_minor_only
    git commit -qam "Release v$next

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
  else
    next="$current"
  fi
  tag="v$next"
  git rev-parse -q --verify "refs/tags/$tag" >/dev/null && { echo "$tag already exists" >&2; exit 1; }
  prev=$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)
  git tag -a "$tag" -m "DeadTune $tag"
  git push -q origin main "$tag"
fi

sha=$(git rev-parse HEAD)
short=${sha::7}
branch=$(git rev-parse --abbrev-ref HEAD)
ver=$(version)
if [ "$mode" = testing ] && [ -z "$(git branch -r --contains "$sha" 2>/dev/null)" ]; then
  echo "HEAD $short is not on any remote branch; push first (the release tag points at it)" >&2
  exit 1
fi
[ -n "$(git status --porcelain --untracked-files=no)" ] && echo "note: uncommitted changes are not included; building $short"
case "$mode" in
minor|first) zip="deadtune-v$ver-windows-x64.zip" release="v$ver" channel=stable ;;
*) zip=deadtune-windows-x64.zip release=testing channel=testing ;;
esac
exe=deadtune-windows-x64.exe

out="$root/target/release-local"
src="$out/src"
rm -rf "$src" && mkdir -p "$src"
git archive "$sha" | tar -x -C "$src"
export CARGO_TARGET_DIR="$out/cargo"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export DEADTUNE_COMMIT="$sha"
(cd "$src" && cargo build --release --target $target -p dt-gui -p dt-cli --features dt-gui/fetch,dt-cli/fetch \
  && cargo build --release --target $target -p dt-core --example hud_build)

rel="$CARGO_TARGET_DIR/$target/release"
stage="$out/stage"
rm -rf "$stage" && mkdir -p "$stage/tools"
cp "$rel/deadtune.exe" "$src/docs/testing-windows.md" "$stage/"
cp "$rel/deadtune-cli.exe" "$rel/examples/hud_build.exe" "$src"/crates/dt-core/examples/*.sample.toml "$stage/tools/"
echo "DeadTune $ver, $branch $sha (local $target build)" > "$stage/BUILD.txt"
rm -f "$out/$zip" && (cd "$stage" && zip -qr "$out/$zip" .)
(cd "$out" && shasum -a 256 "$zip" > "$zip.sha256")

update=("$out/$exe" "$out/$exe.minisig" "$out/latest.json")
rm -f "${update[@]}"
cp "$rel/deadtune.exe" "$out/$exe"
minisign -S -s "$key" -m "$out/$exe" -x "$out/$exe.minisig" -t "deadtune $ver $sha windows-x64" </dev/null
minisign -Vm "$out/$exe" -x "$out/$exe.minisig" -p "$pubkey" \
  || { echo "signature does not verify with $pubkey: wrong release key?" >&2; exit 1; }
(cd "$src" && cargo run -q -p dt-core --example make_manifest -- --channel "$channel" \
  --version "$ver" --commit "$sha" --published "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --notes-url "https://github.com/simulieren/deadtune/releases/tag/$release" \
  --asset "windows-x64=https://github.com/simulieren/deadtune/releases/download/$release/$exe,$out/$exe,$out/$exe.minisig") \
  > "$out/latest.json"
ls -l "$out/$zip" "${update[@]}"

case "$mode" in
--no-upload) exit 0 ;;
testing)
  gh release delete testing --cleanup-tag --yes 2>/dev/null || true
  gh release create testing "$out/$zip" "$out/$zip.sha256" "${update[@]}" --prerelease --target "$sha" \
    --title "Testing build $short (v$ver+, $branch, local)" \
    --notes "Untested in-between build of \`$branch\` at $sha, after v$ver. Get it on Windows with \`scripts/get-testing.ps1\`."
  echo "On Windows:  powershell -ExecutionPolicy Bypass -File scripts\\get-testing.ps1" ;;
minor|first)
  notes="$out/notes.md"
  { echo "DeadTune $tag for Windows x64. Download \`$zip\`, unzip, run \`deadtune.exe\`."; echo
    echo "## Changes"; git log --no-merges --format='- %s' ${prev:+$prev..}"$tag" | grep -v '^- Release v' | head -60; } > "$notes"
  gh release create "$tag" "$out/$zip" "$out/$zip.sha256" "${update[@]}" --verify-tag --latest --title "DeadTune $tag" --notes-file "$notes"
  echo "On Windows:  powershell -ExecutionPolicy Bypass -File scripts\\get-testing.ps1 -Tag $tag" ;;
esac
