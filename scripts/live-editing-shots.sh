#!/usr/bin/env bash
# Screenshots of the HUD page's "Live editing" switch, on and off, while the preview is live,
# at 1280x800 and 1600x1000, compressed for reading. Output: target/shots/live-editing/
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
fake="$root/target/fake-deadlock"
game="$fake/steamapps/common/Deadlock"
[ -f "$game/game/citadel/pak01_dir.vpk" ] || scripts/fake-install.sh "$fake" >/dev/null
out="$root/target/shots/live-editing"
data="$root/target/live-editing-data"
mkdir -p "$out" "$data"
grep -qs '^onboarded' "$data/settings.toml" || echo 'onboarded = true' >>"$data/settings.toml"
cargo build -q --profile fast -p dt-gui
bin="$root/target/fast/deadtune"

for editing in on off; do
  for size in 1280x800 1600x1000; do
    raw="$out/.$editing-$size.png"
    DEADTUNE_DATA_DIR="$data" DEADTUNE_SECTION=hud DEADTUNE_FAKE_RUNNING=1 \
      DEADTUNE_FAKE_LIVE_HUD=live DEADTUNE_LIVE_EDITING="$editing" \
      DEADTUNE_SCREENSHOT="$raw" "$bin" --game-dir "$game" --size "$size" >/dev/null 2>&1
    sips -s format jpeg -s formatOptions 80 "$raw" --out "$out/$editing-$size.jpg" >/dev/null
    rm -f "$raw"
    echo "$out/$editing-$size.jpg"
  done
done
