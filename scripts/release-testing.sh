#!/usr/bin/env bash
# Builds a Windows testing release from a pushed branch and waits for it.
# Usage: scripts/release-testing.sh [branch]   (default: current branch)
set -euo pipefail
ref="${1:-$(git branch --show-current)}"
if [ "$(git rev-parse "$ref")" != "$(git rev-parse "origin/$ref" 2>/dev/null)" ]; then
  echo "origin/$ref is not at local $ref; push first" >&2
  exit 1
fi
gh workflow run release.yml --ref "$ref"
sleep 5
run=$(gh run list --workflow release.yml --branch "$ref" --event workflow_dispatch -L 1 --json databaseId -q '.[0].databaseId')
gh run watch "$run" --exit-status
echo
echo "On Windows:  powershell -ExecutionPolicy Bypass -File scripts\\get-testing.ps1"
gh release view testing --json url -q .url
