#!/usr/bin/env bash
# Cross-compiles the Windows zip on this machine (MinGW, no Actions minutes) and publishes it.
#   scripts/release-local.sh              rolling `testing` prerelease from committed HEAD
#   scripts/release-local.sh --no-upload  build the zip in target/release-local only
#   scripts/release-local.sh minor        semver release: bump 0.Y.0 -> 0.(Y+1).0, commit, tag, publish
#   scripts/release-local.sh first        publish the current version as its first release (no bump)
# Versioning is minor-only for now: every release is 0.Y.0 (enforced by dt-core's version test).
# Needs: brew install mingw-w64; rustup target add x86_64-pc-windows-gnu; gh auth login.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
target=x86_64-pc-windows-gnu
mode="${1:-testing}"
case "$mode" in testing|--no-upload|minor|first) ;; *)
  echo "usage: $0 [--no-upload|minor|first]  (only minor releases are allowed for now)" >&2; exit 2 ;;
esac

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
case "$mode" in minor|first) zip="deadtune-v$ver-windows-x64.zip" ;; *) zip=deadtune-windows-x64.zip ;; esac

out="$root/target/release-local"
src="$out/src"
rm -rf "$src" && mkdir -p "$src"
git archive "$sha" | tar -x -C "$src"
export CARGO_TARGET_DIR="$out/cargo"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
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
ls -l "$out/$zip"

case "$mode" in
--no-upload) exit 0 ;;
testing)
  gh release delete testing --cleanup-tag --yes 2>/dev/null || true
  gh release create testing "$out/$zip" "$out/$zip.sha256" --prerelease --target "$sha" \
    --title "Testing build $short (v$ver+, $branch, local)" \
    --notes "Untested in-between build of \`$branch\` at $sha, after v$ver. Get it on Windows with \`scripts/get-testing.ps1\`."
  echo "On Windows:  powershell -ExecutionPolicy Bypass -File scripts\\get-testing.ps1" ;;
minor|first)
  notes="$out/notes.md"
  { echo "DeadTune $tag for Windows x64. Download \`$zip\`, unzip, run \`deadtune.exe\`."; echo
    echo "## Changes"; git log --no-merges --format='- %s' ${prev:+$prev..}"$tag" | grep -v '^- Release v' | head -60; } > "$notes"
  gh release create "$tag" "$out/$zip" "$out/$zip.sha256" --verify-tag --title "DeadTune $tag" --notes-file "$notes"
  echo "On Windows:  powershell -ExecutionPolicy Bypass -File scripts\\get-testing.ps1 -Tag $tag" ;;
esac
