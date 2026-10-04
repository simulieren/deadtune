#!/usr/bin/env bash
# Rebuilds crates/dt-gui/assets/fonts/ from the official Inter and Hack releases. egui's bundled
# fonts are off (eframe without `default_fonts`), so these subsets are every glyph the GUI has.
# Needs pyftsubset (pip install fonttools). Usage: scripts/subset-fonts.sh [inter-version] [hack-version]
# A glyph the GUI draws but the subset lacks fails theme::tests::every_glyph_renders_in_inter;
# add its codepoint to the ranges and rerun.
set -euo pipefail
inter="${1:-4.1}"
hack="${2:-3.003}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/crates/dt-gui/assets/fonts"
INTER_UNICODES="U+0000-00FF,U+2000-206F,U+2190-21FF,U+2212,U+2605,U+2713,U+2715"
# Monospace shows convar names, diffs and file paths: keep Latin Extended and box drawing.
HACK_UNICODES="U+0000-024F,U+2000-206F,U+2190-21FF,U+2212,U+2500-257F"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
curl -sSL -o "$work/inter.zip" "https://github.com/rsms/inter/releases/download/v$inter/Inter-$inter.zip"
unzip -q "$work/inter.zip" -d "$work/inter"
for weight in Regular SemiBold; do
  pyftsubset "$work/inter/extras/ttf/Inter-$weight.ttf" \
    --unicodes="$INTER_UNICODES" --layout-features=kern --no-hinting \
    --output-file="$out/Inter-$weight.ttf"
done
cp "$work/inter/LICENSE.txt" "$out/OFL.txt"
curl -sSL -o "$work/hack.zip" "https://github.com/source-foundry/Hack/releases/download/v$hack/Hack-v$hack-ttf.zip"
unzip -q "$work/hack.zip" -d "$work/hack"
pyftsubset "$work/hack/ttf/Hack-Regular.ttf" \
  --unicodes="$HACK_UNICODES" --layout-features= --no-hinting \
  --output-file="$out/Hack-Regular.ttf"
curl -sSL -o "$out/Hack-LICENSE.md" "https://raw.githubusercontent.com/source-foundry/Hack/v$hack/LICENSE.md"
ls -l "$out"
