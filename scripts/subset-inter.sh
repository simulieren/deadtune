#!/usr/bin/env bash
# Rebuilds crates/dt-gui/assets/fonts/Inter-{Regular,SemiBold}.ttf from the official Inter release.
# Needs pyftsubset (pip install fonttools). Usage: scripts/subset-inter.sh [version]  (default 4.1)
# A glyph the GUI draws but the subset lacks fails theme::tests::every_glyph_renders_in_inter;
# add its codepoint to UNICODES and rerun.
set -euo pipefail
version="${1:-4.1}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/crates/dt-gui/assets/fonts"
UNICODES="U+0000-00FF,U+2000-206F,U+2190-21FF,U+2212,U+2605,U+2713,U+2715"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
curl -sSL -o "$work/inter.zip" "https://github.com/rsms/inter/releases/download/v$version/Inter-$version.zip"
unzip -q "$work/inter.zip" -d "$work/inter"
for weight in Regular SemiBold; do
  pyftsubset "$work/inter/extras/ttf/Inter-$weight.ttf" \
    --unicodes="$UNICODES" --layout-features=kern --no-hinting \
    --output-file="$out/Inter-$weight.ttf"
done
cp "$work/inter/LICENSE.txt" "$out/OFL.txt"
ls -l "$out"
