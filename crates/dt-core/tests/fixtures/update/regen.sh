#!/usr/bin/env bash
# Regenerates the update test fixtures with throwaway keys. No secret key survives this script.
set -euo pipefail
cd "$(dirname "$0")"
keys="$(mktemp -d)"
trap 'rm -rf "$keys"' EXIT
minisign -G -W -f -p test.pub -s "$keys/test.key" >/dev/null
minisign -G -W -f -p "$keys/other.pub" -s "$keys/other.key" >/dev/null
printf 'deadtune update fixture: pretend this is deadtune.exe\n' > asset.bin
minisign -S -s "$keys/test.key" -m asset.bin -x asset.bin.minisig -t "deadtune 9.9.9 abc1234 windows-x64" >/dev/null
minisign -S -s "$keys/test.key" -m asset.bin -x asset.bin.v998.minisig -t "deadtune 9.9.8 abc1234 windows-x64" >/dev/null
minisign -S -s "$keys/other.key" -m asset.bin -x asset.bin.otherkey.minisig -t "deadtune 9.9.9 abc1234 windows-x64" >/dev/null
