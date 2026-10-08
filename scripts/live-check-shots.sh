#!/usr/bin/env bash
# Screenshots of every "Check live preview" state at 1280x800 and 1600x1000, compressed for
# reading. Usage: scripts/live-check-shots.sh [state ...] (default: all). Output: target/shots/live-check/
# States: showing shown asking (the visible test) and the dt_core::hud::live_check::sample
# names; `system:<state>` shows it on the System check page instead of the HUD page.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
states=("$@")
if [ ${#states[@]} -eq 0 ]; then
  states=(showing shown asking works old_pak pending no_condebug script_error no_sliders sliders not_followed not_seen system:old_pak)
fi
fake="$root/target/fake-deadlock"
game="$fake/steamapps/common/Deadlock"
[ -f "$game/game/citadel/pak01_dir.vpk" ] || scripts/fake-install.sh "$fake" >/dev/null
out="$root/target/shots/live-check"
data="$root/target/live-check-data"
mkdir -p "$out" "$data"
grep -qs '^onboarded' "$data/settings.toml" || echo 'onboarded = true' >>"$data/settings.toml"
cargo build -q --profile fast -p dt-gui
bin="$root/target/fast/deadtune"

for state in "${states[@]}"; do
  section=hud
  case "$state" in old_pak|pending|*:old_pak) hud=stale ;; no_condebug|script_error|no_sliders) hud=waiting_long ;; *) hud=live ;; esac
  name="$state"
  case "$state" in
    system:*) section=system; state="${state#system:}"; name="system-$state" ;;
  esac
  for size in 1280x800 1600x1000; do
    raw="$out/.$name-$size.png"
    DEADTUNE_DATA_DIR="$data" DEADTUNE_SECTION="$section" DEADTUNE_FAKE_RUNNING=1 \
      DEADTUNE_FAKE_WINDOWS=1 DEADTUNE_FAKE_LIVE_HUD="$hud" DEADTUNE_FAKE_LIVE_CHECK="$state" \
      DEADTUNE_SCREENSHOT="$raw" "$bin" --game-dir "$game" --size "$size" >/dev/null 2>&1
    sips -s format jpeg -s formatOptions 80 "$raw" --out "$out/$name-$size.jpg" >/dev/null
    rm -f "$raw"
    echo "$out/$name-$size.jpg"
  done
done
