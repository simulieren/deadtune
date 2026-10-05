#!/usr/bin/env bash
# Builds DeadTune with the quick `fast` profile and opens it on a fake Deadlock install.
# Usage: scripts/app.sh [--images <Save all images folder>] [--fresh] [-- extra deadtune args]
#   --images  draw the HUD previews and the UI images page from an exported folder
#             (default: $DEADTUNE_PREVIEW_IMAGES)
#   --fresh   rebuild the fake install and start with an empty data folder
# Data lives in target/app-data, the fake game in target/fake-deadlock; the previous window closes.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
images="${DEADTUNE_PREVIEW_IMAGES:-}"
fresh=0
extra=()
while [ $# -gt 0 ]; do
  case "$1" in
    --images) images="$2"; shift 2 ;;
    --fresh) fresh=1; shift ;;
    --) shift; extra=("$@"); break ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

fake="$root/target/fake-deadlock"
data="$root/target/app-data"
game="$fake/steamapps/common/Deadlock"
if [ "$fresh" = 1 ]; then rm -rf "$fake" "$data"; fi
[ -f "$game/game/citadel/pak01_dir.vpk" ] || scripts/fake-install.sh "$fake" >/dev/null
mkdir -p "$data"
grep -qs '^onboarded' "$data/settings.toml" || echo 'onboarded = true' >>"$data/settings.toml"

cargo build -q --profile fast -p dt-gui
bin="$root/target/fast/deadtune"
pkill -f "^$bin" 2>/dev/null || true

env=(DEADTUNE_DATA_DIR="$data")
if [ -n "$images" ]; then
  env+=(DEADTUNE_PREVIEW_IMAGES="$images" DEADTUNE_IMAGES_FROM="$images")
fi
env "${env[@]}" nohup "$bin" --game-dir "$game" ${extra[@]+"${extra[@]}"} >"$root/target/app.log" 2>&1 &
echo "DeadTune is open (log: target/app.log)"
