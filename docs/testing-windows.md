# Testing DeadTune on Windows

## Start DeadTune

1. Unzip the build into any folder, for example `Documents\DeadTune`.
2. Double-click `deadtune.exe`.
3. If Windows shows "Windows protected your PC", click **More info**, then **Run anyway**. The app is not code-signed yet, so Windows shows this once per build.
4. DeadTune finds Deadlock through Steam. If it can't, it asks you for the Deadlock folder, usually `C:\Program Files (x86)\Steam\steamapps\common\Deadlock`.
5. On first launch, pick a starting preset (or "Keep my current settings") and press **Apply**. After that you land on the main screen: Overview with the goal cards, settings sections in the sidebar, HUD, and Safety & setup.

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

Versioned releases (`v0.1.0`, `v0.2.0`, ...) are stable checkpoints; the `testing` build is whatever is newest in between:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\get-testing.ps1 -Tag latest -Run   # newest version
powershell -ExecutionPolicy Bypass -File scripts\get-testing.ps1 -Tag v0.1.0 -Run   # a specific one
```

Each build lands in `%USERPROFILE%\DeadTune-testing\<tag or commit>`, so older builds stay for comparison. The script checks the SHA-256 and unblocks the files.

### Updates

Every release (testing and versioned) also publishes `deadtune-windows-x64.exe`, its `.minisig` signature and `latest.json`. That is what the in-app updater downloads; it checks the signature against the key built into the app before replacing `deadtune.exe`. To test it, run an older build and publish a newer testing build. How releases are signed is in `docs/releasing.md`.

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
scripts/release-local.sh minor        # bump 0.Y.0 -> 0.(Y+1).0, tag, publish a versioned release
```

It cross-compiles with MinGW (`brew install mingw-w64`) and uses no GitHub Actions minutes. HEAD must be pushed. Versioning is semver, minor-only for now: every release is `0.Y.0`, and a test fails the build otherwise. A tag `v*` also makes a release through `.github/workflows/release.yml` when Actions is enabled; it checks the tag matches `Cargo.toml`.

## Developer: HUD test without the GUI (phase H0 in docs/plan-hud.md)

The HUD tab in the app is the normal way. `tools\hud_build.exe` runs the same code from the command line if you need to isolate a problem. Back up `game\citadel\gameinfo.gi` first.

```powershell
.\tools\hud_build.exe "C:\Program Files (x86)\Steam\steamapps\common\Deadlock" tools\hud_layout.sample.toml --install
```

It prints the planned change, installs `game\citadel\addons\pak77_dir.vpk`, and shows the one line `gameinfo.gi` needs (`Game citadel/addons`) if it is missing. Add that line, launch the game, and note what moved. Run it with a layout that has no edits to remove the addon again.

## Verification checklist

Work through this on the gaming PC, top to bottom. Each item is something we could not prove on the Mac. Tick it, or write what happened next to it. Phase 0 items from `PLAN.md` section 8 are marked **P0-n**, HUD spikes from `docs/plan-hud.md` are marked **H0-n**.

### 1. Install and first launch
- [ ] `deadtune.exe` starts from Explorer with no console window behind it.
- [ ] Release `deadtune.exe` size (plan target: 5 MB or less).
- [ ] DeadTune finds Deadlock on its own, including when the game is in a second Steam library.
- [ ] Safety & setup > **Check setup** is all green, or every yellow/red row has a fix that makes sense. In particular "Game archive (HUD)" reads the real `pak01_dir.vpk`, and "Write cfg folder" passes under Program Files.
- [ ] Byte check: an empty edit leaves the real stock `gameinfo.gi` identical (Check setup's "Lossless edit" row), and its line endings are reported as Windows (CRLF).

### 2. First apply
- [ ] Pick a goal card (e.g. More FPS), press Apply. `gameinfo.gi` and `video.txt` change; the backups list in Advanced > Backups shows the originals.
- [ ] Both files still use CRLF line endings after Apply.
- [ ] Deadlock launches and the change is visible in game (e.g. lower view distance). The game does not reset `video.txt` and keeps its VendorID / DeviceID / Version lines.
- [ ] Switching to a preset drops the stock nested `rate { min default max }` block from ConVars when the preset lacks it. Check nothing network-related misbehaves (open question).
- [ ] Undo last change and Restore original game files both bring the files back byte for byte.

### 3. Live changes while playing

How it works now: DeadTune writes `cfg\deadtune_boot.cfg` (binds F8, sets the profile's live convars, prints `DEADTUNE_BOOT`) and starts the game with `+exec deadtune_boot -condebug`. Every push writes `cfg\deadtune_live.cfg` ending in `echo DEADTUNE_ACK <nonce> <n>`, one bare convar name per changed setting (the console prints its value) and `echo DEADTUNE_END <nonce>`. DeadTune tails the game's console log and ticks each setting off. The log location is the big unknown; DeadTune watches `game\citadel\console.log` and `game\citadel\deadtune_console.log` plus the same names one and two folders up.

- [ ] **P0-1a**: Close Deadlock. In DeadTune press **Launch Deadlock** (sidebar). Steam may ask to confirm the launch options; accept. Once in the main menu, open the console (F7): is there a line `DEADTUNE_BOOT 0.1.0`? Does Safety & setup > Instant changes show step 1 ticked? If not, note whether `game\citadel\console.log` exists at all (that is `-condebug`) and whether `con_logfile` printed `Unknown command` in the console.
- [ ] **P0-1b**: Safety & setup > **Send test**, then press F8 in game with the console closed. The card should go from "Waiting for Deadlock: press F8 in game (Ns)" to a green "Deadlock applied 1 of 1 at HH:MM" and step 2 ticks. Hover the green line: it lists `fps_max = <value>` and the raw console lines. Copy those raw lines into your notes; they tell us the real output format for a bare convar query.
- [ ] **P0-1c**: Change the FPS limit (Performance), press F8 in game: the value changes, and the action bar shows "Deadlock applied 1 of 1". Then set a Restart-class convar in Advanced (e.g. `ai_foot_sweep_enable`), **Push live**, press F8: the pending panel should say "0 applied, 1 needs a restart (...)" and the hover text shows what the console said (we expect `Unknown command`; write down the exact wording).
- [ ] **P0-1d**: Press F8 without a push in flight, and with the console open: no crash, status unchanged.
- [ ] Timeout: close Deadlock, change the FPS limit, wait 10 s: "No reply from Deadlock after 10 s" with the checklist.
- [ ] From Steam instead: quit Deadlock, copy the Launch Options text from Safety & setup (`+exec deadtune_boot -condebug`), paste it into Steam > Deadlock > Properties > General > Launch Options, start the game from Steam. Step 1 should tick again.
- [ ] CLI: `deadtune-cli push fps_max=120 --wait 15`, press F8 in game: it prints `fps_max = 120` and `Deadlock applied 1 of 1.`
- [ ] The Copy buttons' text pastes into the F7 console / Steam.
- [ ] Writing `deadtune_live.cfg` while the game has it open works (or fails with a clear message).
- [ ] **P0-2**: launch with `-netconport 2121` (no `-tools`). Advanced > Settings > netcon **Probe**: does it connect? Does a push arrive? Repeat with `-tools`.
- [ ] **P0-4**: in Hideout, tick Sandbox and push 3 or 4 "Cheat" convars. Which apply live? Do "Restart" ones get rejected?
- [ ] **P0-6**: change a menu video setting in game. Does the game re-read `video.txt` without a restart?
- [ ] Mini window: opens from the sidebar's "Mini window", stays on top of a borderless-windowed game, and Expand returns to the main window.

### 4. Ranked
- [ ] **P0-3**: with modified ConVars, can you queue matchmaking? With Ranked-safe mode on? With only the HUD addon installed (**H0-4**)?

### 5. Restart loop
- [ ] Apply + relaunch (Advanced): closes `deadlock.exe`, starts it through Steam, the countdown runs and "restart pending" clears.
- [ ] Launch options with spaces or `+` survive the `steam://run` URL; note whether Steam shows a "launch with these options?" dialog.
- [ ] **P0-5**: is there a `+map <name>` launch option that drops straight into sandbox/hideout?
- [ ] DeadTune shows "Deadlock running" / "closed" correctly.

### 6. Game updates
- [ ] After a Steam update or "Verify integrity of game files", DeadTune shows the "Deadlock updated" banner and Re-apply works.

### 7. HUD (`docs/plan-hud.md` phase H0)
- [ ] DeadTune's addon paks leave the VPK MD5 section zeroed (QoL Lite ships the same way). Confirm the game mounts them; Source2Viewer's verify step will report a checksum mismatch, which is expected.
- [ ] **H0-1**: move the minimap in the HUD tab, Apply, launch. Does the generated `hud.vcss_c` load at all?
- [ ] **H0-2, H0-7, H0-8**: do moves, scale and the ammo panel take effect; does the minimap scale hold in matches?
- [ ] **H0-3**: with QoL Lite or QOL Lock installed too, which addon wins? Check setup should warn about the conflict.
- [ ] **H0-5, H0-6**: crosshair convars live from console; which state shows `#hud_signature`.
- [ ] **H0-9**: screenshot the vanilla HUD at 1920x1080 and compare with the HUD tab's preview boxes.

### 8. Benchmark
- [ ] **P0-7**: capture 3 identical 60 s runs with PresentMon, import each in Advanced > Bench. Variance under 3%? Note which CSV columns your PresentMon version writes (`MsBetweenPresents` or `FrameTime`).

### 9. Edge cases
- [ ] Apply while the game is running (writes go to disk for next launch; message says so).
- [ ] Apply while antivirus or the game holds `gameinfo.gi` open: a friendly "Access is denied" style message, and no half-written file.
- [ ] Advanced > Settings "Open folder" buttons open Explorer at backups, data and screenshots.
- [ ] Laptop: power source shows AC vs battery correctly, and the auto power profile switches before launch.
- [ ] Phone remote (built with `--features remote`): reachable from a phone through Windows Firewall, on the right network adapter.

### 10. Texture downscale (developer example, no GUI yet)

DeadTune builds its own low-VRAM textures from the game's files: it drops the largest mip levels of each `.vtex_c` in `pak01` and writes the smaller copies into `game\citadel\addons\pak78_dir.vpk` (plus `pak78_000.vpk`, `pak78_001.vpk`, ... when the output passes 256 MB). The game files are never written, so there is no backup to make. Deleting those `pak78` files restores full quality.

Build and run from a checkout (needs Rust):

```powershell
cargo run --release -p dt-core --example texture_downscale -- "C:\Program Files (x86)\Steam\steamapps\common\Deadlock" --list
cargo run --release -p dt-core --example texture_downscale -- "C:\Program Files (x86)\Steam\steamapps\common\Deadlock" --install
```

`--quarter` drops two levels instead of one. `--all` includes UI and unclassified textures, `--lighting` includes lightmaps and other baked lighting (the suspected cause of the bright white map). The run prints how many textures it reduced, what it skipped and why, and reminds you if `gameinfo.gi` lacks `Game citadel/addons`.

- [ ] `--list` runs and the category split looks right: hero textures under "heroes", particles under "particles", HUD images under "ui". Paste the output (or the surprising lines) into the notes.
- [ ] `--install` finishes. Note the time, the "replaced X of originals with Y of copies" line, and the file sizes in `game\citadel\addons`.
- [ ] If any texture from pak01 is reported as "malformed", send one such file: that is the compressed-mip layout we could not test on the Mac.
- [ ] Deadlock launches with the addon mounted (console shows no texture errors at map load).
- [ ] Textures look half-resolution up close (props, hero skins) and the map is NOT washed out white. If it is, rerun with `--list` and report which lighting-looking paths were reduced.
- [ ] VRAM drops: compare the GPU memory counter (Task Manager or `mat_texture_list` in the console) with and without the addon in the same spot.
- [ ] No pop-in or textures that never sharpen (texture streaming still works with fewer mips).

### 11. Performance addons (Addons section in the sidebar)

Each addon is one `game\citadel\addons\pakNN_dir.vpk` that DeadTune writes and removes (its preferred number is 71 to 76; a number already used by another mod is skipped). `deadtune-cli addons list` shows the same table. The upstream files come from Sqooky's OptimizationLock repository: a build with the `fetch` feature downloads them with a Download button; otherwise download the five `pakNN_dir.vpk` files (or extract the GameBanana archive) and paste the folder into "Import downloaded files". Nothing on the Mac could prove that the game accepts these paks, so every item here is an in-game check.

- [ ] **A-1 Vindicta scope / Sinner light / soul container** (verbatim upstream files at a different pak number): turn one on, Apply, launch. Does the mod work at `pak74`/`pak73`/`pak75` the way it does at its upstream number? Check setup shows "Performance addons: installed: ...".
- [ ] **A-2 Blur disabler**: turn on, Apply, launch. The minimap frame and menus have no blur. Then turn off only "Menus" in its options, Apply: HUD blur stays off, menu blur returns. The pak is generated from your `pak01` (`panorama/styles/citadel_base_styles.vcss_c` with the two `@define`s set to `none`), not copied from upstream, so this is the real test of that generation.
- [ ] **A-3 Blur after an update**: after the next Steam update, Check setup says "Addons after update: built for an older game version" and Apply rebuilds it (the Addons card says "Rebuilds on Apply").
- [ ] **A-4 Particle disabler, all groups**: turn on with everything hidden (the default), Apply, launch, take damage to low health: no red vignette. Compare with upstream `pak02` installed by hand: identical behaviour.
- [ ] **A-5 Particle disabler, some groups**: in Options untick "Low health vignette" and "Damage and death flashes", Apply. The vignette is back; hero debuff effects (for example Infernus burn) stay hidden. The pak holds only the hidden paths plus `materials/debug/debugempty_color_tga_fd967415.vtex_c`.
- [ ] **A-6 Hero names in the particle list**: the labels with a codename in brackets (bookworm, butcher, druid, familiar, fencer, frank, hijack, priest, punkgoat, synth) and the guessed ones (Lady Geist for `ghost`, Fathom, Doorman, Wrecker, Paradox's `chrono_sphere`) were not verified. Trigger each effect in the sandbox and write the right hero next to the codename; the table is `crates/dt-core/src/addons/particles.rs`.
- [ ] **A-7 Slot collision**: with QoL Lite (`pak01`) or another mod at `pak71` present, enabling the particle disabler lands at the next free number (`pak70`) and Apply never touches the other mod's file. Disabling removes only ours.
- [ ] **A-8 Ranked-safe**: with addons installed, Ranked-safe mode removes every DeadTune pak (HUD too), leaves other mods, and queueing matchmaking still works (**P0-3**). Turning it off brings them back on the next Apply.
- [ ] **A-9 Foreign file**: overwrite one of our paks with any other file. Check setup says "not what DeadTune wrote"; Apply reinstalls at a new number and never deletes the replaced file.
- [ ] **A-10 Texture downscaler from the GUI**: same checks as section 10, started from the Addons card (Build button, progress with Cancel, stats line after). Cancel mid-way leaves no `pak76` files behind. The pak number is 76 here (the example in section 10 uses 78).
- [ ] `--quarter --install` repeats the above with a stronger effect.
- [ ] Delete the `pak78*` files, relaunch: full quality is back.
- [ ] A multi-file output (`pak78_dir.vpk` plus `pak78_000.vpk`) mounts. Half size on a full game should produce one.
- [ ] Matchmaking still queues with the addon mounted (same question as **H0-4**).

### 10. Developer checks (repo checkout on Windows)
- [ ] `cargo test --workspace --all-features` passes, including the Windows-only `push_uses_crlf_on_windows`.
- [ ] With `core.autocrlf=true`, a fresh clone still passes the catalog drift test.
- [ ] `cargo test -p dt-core --features fetch -- --ignored fetch_latest` downloads every preset.
- [ ] `tools\deadtune-cli.exe doctor` matches the GUI's Check setup; its y/N confirm prompt works in cmd and PowerShell.
