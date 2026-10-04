# DL-FOV-Fixer

A tiny Windows **system-tray** app that keeps your **Deadlock FOV** fix applied.

Deadlock has no in-game FOV slider. The community workaround is to add
`"r_aspectratio" "<value>"` to the `ConVars` block of Deadlock's `gameinfo.gi`. A higher value
gives a wider field of view. The catch: **every game update can overwrite that file**, so you have
to redo the edit by hand. This app remembers your chosen value and re-applies it for you.

<p align="center">
  <img src="assets/icon.ico" width="96" alt="DL-FOV-Fixer icon">
</p>

## Install

Download `DL-FOV-Fixer-<version>-setup.exe` from the
[latest release](https://github.com/lukr-99/DL-FOV-Fixer/releases/latest) and run it. It installs
for your Windows user only, with no admin prompt, to `%LOCALAPPDATA%\Programs\DL-FOV-Fixer`, and
adds a Start menu entry.

**Coming from 1.x:** run the installer the same way. It replaces the old `DL-FOV-Fixer.exe` in that
folder and keeps your FOV, your stored tweaks and your settings
(`%APPDATA%\DL-FOV-Fixer\config.json`). A copy of 1.x kept anywhere else can simply be deleted.

**The Windows warning.** The installer is not code-signed, so the first time you run a downloaded
version, SmartScreen shows "Windows protected your PC". Click **More info**, then **Run anyway**.
To check the download is the file the release built, compare its SHA-256 with the `.sha256` file on
the same release:

```powershell
(Get-FileHash .\DL-FOV-Fixer-2.0.0-setup.exe -Algorithm SHA256).Hash.ToLower()
```

Uninstall from **Settings > Apps**. Your settings stay in `%APPDATA%\DL-FOV-Fixer`, so a reinstall
picks them up again.

## What it does

- **Auto-locates** `gameinfo.gi` from your Steam libraries on first run (the Steam path from the
  registry, then `libraryfolders.vdf`). If it can't find it, it asks you to pick the file, and it
  keeps the value already in the file rather than overwrite it.
- Ensures `"r_aspectratio" "<your value>"` is present in the `ConVars` block, **without touching**
  the rest of the file. Nested blocks like `rate` are left exactly as they are, because mangling
  those stops the game from launching.
- **Re-applies within seconds of a game update**, by watching the file, and on a timer as a fallback.
- Waits while the game has the file open, and applies once it's free.
- Makes a one-time backup next to the original: `gameinfo.gi.dlfovfixer.bak`.
- **Updates itself** from this repository's releases. Each release's `manifest.json` is signed with
  the project's own key, and the app installs an update only when that signature and the
  installer's SHA-256 match.

## Status at a glance

The tray icon changes color so you can tell the state without opening the menu:

| Icon | Meaning |
|------|---------|
| 🟢 **Green** | File found and your FOV value is applied. All good. |
| 🟠 **Amber** | Not applied yet, changed by a game update, or the game has the file open. It applies by itself. |
| 🔴 **Red** | A problem: `gameinfo.gi` can't be found or read, or a write failed. |

## Tray menu

| Item | Action |
|------|--------|
| **Apply now** | Write the FOV (and, if enabled, all extra tweaks) into the files. A left click on the icon does the same. |
| **Check file now** | Report whether `gameinfo.gi` currently matches your FOV target. |
| **Set FOV value ▸** | Pick a preset (80 to 115°) or enter a custom `r_aspectratio` value. |
| **Extra tweaks ▸** | Paste or import a config, view or clear stored tweaks, toggle applying them. |
| **Check for updates** / **Install update** | Look for a newer release and install it. |
| **Check updates on start** | Toggle the quiet update check when the app starts. |
| **Open gameinfo.gi** | Open the file in your editor. |
| **Locate gameinfo.gi…** | Manually point the app at the file. |
| **Apply automatically on start** | Toggle applying when the app starts and after game updates. |
| **Start with Windows** | Toggle launch at sign-in (per-user `Run` key). |
| **Theme ▸** | Same as Windows, Light or Dark. |
| **About** / **Quit** | What it is doing, and exit. |

## Extra tweaks: paste a whole config

FOV isn't the only thing an update wipes. **Extra tweaks ▸ Paste / import config…** opens a box
where you can paste someone's whole config. The app stores it and re-applies it alongside the FOV,
so a game update can't blow away your setup.

Keys are routed automatically **by shape**, so there is no need to keep the section headers:

| Key looks like | Goes to |
|----------------|---------|
| `setting.*` | `cfg/video.txt` |
| `PascalCase` (for example `GpuLightBinner`) | `gameinfo.gi` → `SceneSystem` |
| anything else (for example `r_*`, `cl_*`, `lb_*`) | `gameinfo.gi` → `ConVars` |

You can paste bare (`r_directlighting 0`) or quoted (`"r_directlighting" "0"`) pairs, with `//`
comments and decorative headers. All of that is ignored, and `r_aspectratio` is treated as your FOV.
Merging is **surgical**: existing keys are updated in place (comments kept), new keys are added at
the top of the block, and anything that already exists as a nested sub-block (for example `rate`,
`speaker_config`) is left untouched, keys inside it included, so the game still launches.

> **Note on video settings:** modern Deadlock keeps video settings in `.vcfg` files and there may be
> no `video.txt`. The app will create `cfg/video.txt` from any `setting.*` keys you paste, but
> whether the game reads it can vary. The `gameinfo.gi` ConVars and SceneSystem tweaks are the
> reliable part.

## FOV reference

`r_aspectratio` is not degrees; it scales the rendered aspect ratio. Approximate mapping (from
community testing, about `28 × value + 31`):

| r_aspectratio | ≈ FOV |
|---------------|-------|
| 1.75 | 80° |
| 2.15 | 90° |
| 2.49 | 100° |
| 2.66 | 105° |
| 2.83 | 110° |
| 3.00 | 115° |

Default is `2` (about 87°). Pick whatever feels right. Larger values can distort at the edges.

## Build from source

You need the .NET SDK pinned in `global.json`.

```powershell
dotnet build DL-FOV-Fixer.slnx -c Release
dotnet test --solution DL-FOV-Fixer.slnx -c Release --no-build
dotnet run --project src/DlFovFixer.App -- --settings C:\Temp\dlfov-test\config.json
```

`--settings` points the app at a scratch config, so a test run never touches your real settings or,
through them, your real game files. [CONTRIBUTING.md](CONTRIBUTING.md) covers the installer and
releasing, and [ARCHITECTURE.md](ARCHITECTURE.md) how the app is built.

## Notes & safety

- Only the keys you set are changed: the FOV plus any tweaks you import. Existing keys are updated in
  place (comments kept), the rest of the file is preserved byte for byte (newlines included), and
  nested sub-blocks are never rewritten.
- A one-time backup (`gameinfo.gi.dlfovfixer.bak`, and `video.txt.dlfovfixer.bak` if applicable) is
  made the first time each file is modified, and it is never replaced. It holds the game version of
  that day. **Do not copy an old backup back after a game update**: it would undo Valve's changes to
  the file and can stop the game from starting. To get a clean file, use Steam's **Verify integrity
  of game files** instead, then let the app apply your FOV again.
- This edits your own local game files. It doesn't touch anything online and is unrelated to
  anti-cheat. Use at your own discretion.
- Not affiliated with Valve. "Deadlock" is a trademark of Valve Corporation.

See [CHANGELOG.md](CHANGELOG.md) for what changed in each version.

## License

[PolyForm Noncommercial 1.0.0](LICENSE.md): free for personal and non-commercial use; selling or other commercial use requires permission.
