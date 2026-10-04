#!/usr/bin/env bash
# Cross-compiles the Windows testing zip on this machine from committed HEAD and
# replaces the GitHub `testing` prerelease (release storage only, no Actions minutes).
# Needs: brew install mingw-w64; rustup target add x86_64-pc-windows-gnu; gh auth login.
# Usage: scripts/release-local.sh [--no-upload]
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
target=x86_64-pc-windows-gnu
zip=deadtune-windows-x64.zip
sha=$(git rev-parse HEAD)
short=${sha::7}
branch=$(git rev-parse --abbrev-ref HEAD)
upload=1
[ "${1:-}" = "--no-upload" ] && upload=0

if [ $upload = 1 ] && [ -z "$(git branch -r --contains "$sha" 2>/dev/null)" ]; then
  echo "HEAD $short is not on any remote branch; push first (the release tag points at it)" >&2
  exit 1
fi
[ -n "$(git status --porcelain --untracked-files=no)" ] && echo "note: uncommitted changes are not included; building $short"

out="$root/target/release-local"
src="$out/src"
rm -rf "$src" && mkdir -p "$src"
git archive "$sha" | tar -x -C "$src"
export CARGO_TARGET_DIR="$out/cargo"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
(cd "$src" && cargo build --release --target $target -p dt-gui -p dt-cli \
  && cargo build --release --target $target -p dt-core --example hud_build)

rel="$CARGO_TARGET_DIR/$target/release"
stage="$out/stage"
rm -rf "$stage" && mkdir -p "$stage"
cp "$rel/deadtune.exe" "$rel/deadtune-cli.exe" "$rel/examples/hud_build.exe" "$stage/"
cp "$src"/crates/dt-core/examples/*.sample.toml "$src/docs/testing-windows.md" "$stage/"
echo "$branch $sha (local $target build)" > "$stage/BUILD.txt"
rm -f "$out/$zip" && (cd "$stage" && zip -qr "$out/$zip" .)
(cd "$out" && shasum -a 256 "$zip" > "$zip.sha256")
ls -l "$out/$zip"

[ $upload = 0 ] && exit 0
gh release delete testing --cleanup-tag --yes 2>/dev/null || true
gh release create testing "$out/$zip" "$out/$zip.sha256" --prerelease --target "$sha" \
  --title "Testing build $short ($branch, local)" \
  --notes "Local $target build of \`$branch\` at $sha. Get it on Windows with \`scripts/get-testing.ps1\`."
echo "On Windows:  powershell -ExecutionPolicy Bypass -File scripts\\get-testing.ps1"
