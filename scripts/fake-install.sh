#!/usr/bin/env bash
# Builds a fake Deadlock install for running DeadTune on machines without the game.
# Usage: scripts/fake-install.sh [dir]   (default: target/fake-deadlock)
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
dir="${1:-$root/target/fake-deadlock}"
citadel="$dir/steamapps/common/Deadlock/game/citadel"
mkdir -p "$citadel/cfg" "$dir/userdata/123/760/remote/1422450/screenshots"
cp "$root/research/configs/OptimizationLock/clean gameinfo.gi/gameinfo.gi" "$citadel/gameinfo.gi"
cp "$root/research/configs/OptimizationLock/test_cfg/video.txt" "$citadel/cfg/video.txt"
(cd "$root" && cargo run -q -p dt-core --example fake_pak01 -- "$citadel/pak01_dir.vpk" >&2)
cat > "$dir/steamapps/appmanifest_1422450.acf" <<ACF
"AppState"
{
	"appid"		"1422450"
	"name"		"Deadlock"
	"buildid"		"20261004"
	"installdir"		"Deadlock"
}
ACF
echo "$dir/steamapps/common/Deadlock"
