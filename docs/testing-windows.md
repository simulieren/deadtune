# Testing DeadTune on Windows

## Get a build

On the Mac, push the branch and run:

```sh
scripts/release-testing.sh            # current branch
scripts/release-testing.sh main       # or any pushed branch
```

GitHub Actions builds Windows x64 and replaces the `testing` prerelease (about 5 to 10 minutes). It prints the release URL when done.

On Windows, one-time setup:

```powershell
winget install GitHub.cli
gh auth login
```

Then, from a checkout of the repo (or with just the script copied over):

```powershell
powershell -ExecutionPolicy Bypass -File scripts\get-testing.ps1        # opens the folder
powershell -ExecutionPolicy Bypass -File scripts\get-testing.ps1 -Run   # starts deadtune.exe
```

Each build lands in `%USERPROFILE%\DeadTune-testing\<commit>`, so older builds stay for comparison. The script checks the SHA-256 and unblocks the files. SmartScreen may still warn because the exe is unsigned: More info, then Run anyway.

A tag `v*` pushed to GitHub makes a normal release with the same zip.

## What is in the zip

| File | Use |
|---|---|
| `deadtune.exe` | GUI |
| `deadtune-cli.exe` | CLI |
| `hud_build.exe` | Builds the HUD addon from a layout file (HUD phase H0 tests) |
| `hud_layout.sample.toml` | Example layout: minimap 130%, top bar 20px down, passive items shown, chat at 60% |
| `BUILD.txt` | Branch and commit |

## HUD test (phase H0 in docs/plan-hud.md)

Back up `game\citadel\gameinfo.gi` first.

```powershell
.\hud_build.exe "C:\Program Files (x86)\Steam\steamapps\common\Deadlock" hud_layout.sample.toml --install
```

It prints the planned change, installs `game\citadel\addons\pak77_dir.vpk`, and shows the one line `gameinfo.gi` needs (`Game citadel/addons`) if it is missing. Add that line, launch the game, and note what moved. Run it with a layout that has no edits to remove the addon again.
