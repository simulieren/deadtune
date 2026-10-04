# Testing DeadTune on Windows

## Start DeadTune

1. Unzip the build into any folder, for example `Documents\DeadTune`.
2. Double-click `deadtune.exe`.
3. If Windows shows "Windows protected your PC", click **More info**, then **Run anyway**. The app is not code-signed yet, so Windows shows this once per build.
4. DeadTune finds Deadlock through Steam. If it can't, it asks you for the Deadlock folder, usually `C:\Program Files (x86)\Steam\steamapps\common\Deadlock`.

DeadTune backs up `gameinfo.gi` and `video.txt` before it changes anything, and **Ranked-safe mode** puts the game back to stock in one click.

Everything you need is `deadtune.exe`. The `tools` folder is for developers.

## Get a newer build

Builds are published as the `testing` prerelease on GitHub. The repo is private, so downloads go through the GitHub CLI.

One-time setup, in PowerShell:

```powershell
winget install GitHub.cli
gh auth login
```

Then each time, from a checkout of the repo (or with just `scripts\get-testing.ps1` copied over):

```powershell
powershell -ExecutionPolicy Bypass -File scripts\get-testing.ps1        # opens the folder
powershell -ExecutionPolicy Bypass -File scripts\get-testing.ps1 -Run   # starts deadtune.exe
```

Each build lands in `%USERPROFILE%\DeadTune-testing\<commit>`, so older builds stay for comparison. The script checks the SHA-256 and unblocks the files.

## What is in the zip

| File | Use |
|---|---|
| `deadtune.exe` | The app |
| `testing-windows.md` | This guide |
| `tools\deadtune-cli.exe` | Command-line version, for scripting |
| `tools\hud_build.exe` | Developer tool: builds the HUD addon from a layout file |
| `tools\*.sample.toml` | Example layouts for `hud_build` |
| `BUILD.txt` | Branch and commit |

## Making a build (Mac)

```sh
scripts/release-local.sh              # build committed HEAD, replace the testing prerelease
scripts/release-local.sh --no-upload  # build the zip in target/release-local only
```

It cross-compiles with MinGW (`brew install mingw-w64`) and uses no GitHub Actions minutes. HEAD must be pushed. A tag `v*` makes a normal release through `.github/workflows/release.yml` when Actions is enabled.

## Developer: HUD test without the GUI (phase H0 in docs/plan-hud.md)

Until the GUI's HUD tab lands, `tools\hud_build.exe` exercises the same code. Back up `game\citadel\gameinfo.gi` first.

```powershell
.\tools\hud_build.exe "C:\Program Files (x86)\Steam\steamapps\common\Deadlock" tools\hud_layout.sample.toml --install
```

It prints the planned change, installs `game\citadel\addons\pak77_dir.vpk`, and shows the one line `gameinfo.gi` needs (`Game citadel/addons`) if it is missing. Add that line, launch the game, and note what moved. Run it with a layout that has no edits to remove the addon again.
