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

DeadTune updates itself. It checks once a day and shows a banner when a new version is out; **Update and restart** downloads it, checks its signature and swaps the exe. To get every in-between build, set the update channel to **Testing** in Safety & setup (simple view) or the Settings tab (advanced view).

Builds are also published on the [Releases page](https://github.com/simulieren/deadtune/releases): versioned releases plus a rolling `testing` prerelease. The script below fetches them from PowerShell. It uses the GitHub CLI; one-time setup:

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
scripts/release-local.sh --no-upload  # build the zip, signed exe and latest.json in target/release-local only
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
- [ ] Practice mode (section 14): is queueing refused with it on, and does Ranked-safe mode make queueing work again?

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

### 7b. Top bar page (research/hud/top-bar/NOTES.md)

Every option here is a CSS rule appended to the game's own `citadel_hud_top_bar.vcss_c`; the three Extras also rebuild `citadel_hud_top_bar.vxml_c` as a text layout and add `panorama/scripts/deadtune/top_bar.vjs_c` and `panorama/styles/deadtune/top_bar.vcss_c`. `tools\hud_build.exe <game> tools\hud_top_bar.sample.toml --install` builds the same pak without the GUI.

- [ ] **T-1 Stylesheet loads**: Top bar page, preset "Fight readability", Apply, launch into a sandbox or a match. The top bar looks stock while every enemy is visible. `tools\deadtune-cli.exe addons verify` prints `ok` for `pak77_dir.vpk`.
- [ ] **T-2 Dimming follows vision**: in a match, an enemy whose health bar disappears (out of your team's vision) fades to 20 %; it comes back when seen again. Allies, the dead and you never fade. Then "Desaturate" and "Darken": grey and darkened portraits instead.
- [ ] **T-3 Presentation options**: one at a time, Apply, relaunch: dead hero look (grey, dark, faded), portrait size 85 % and gap 6 px (the bar stays centred), ally and enemy colours (health bars and souls tags change, nothing else), clock, soul lead and rejuvenator rows Compact then Hidden, "Hide kill rows" (the scoreboard loses the team kills and AP rows), "Hide souls tags", "Always show levels". Note anything the game overrides (a rule that does not take).
- [ ] **T-4 Layout loads**: turn on "Spawn timers", Apply, launch through DeadTune (guard on). The game must reach the main menu and a match must show the top bar; `console.log` has no `FATAL` and no line naming `citadel_hud_top_bar`. If the bar is missing or the start fails, the text layout is refused: copy the console lines and note it, the KV3 route in NOTES.md section 2 is the fallback.
- [ ] **T-5 Spawn timers**: two chips under the clock. The powerup chip counts down from 5:00 and reaches 0:00 exactly when the bridge buffs spawn; the rejuvenator chip shows the game's own midboss timer text once it has been killed, "UP" when it is up, and a first-spawn countdown before that. Both turn amber in the last 30 s. Hidden in the hideout and in street brawl.
- [ ] **T-6 Urn soul lead**: a chip between the two team soul totals showing the percentage gap, green at or above +15 % (+10 % after minute 15), red at or below the negative threshold, "--" before the totals exist.
- [ ] **T-7 Purchase popups**: with the shop closed, when a hero on either team buys an item, its name pops under that hero's portrait for 10 s (at most three per hero), bordered weapon orange, vitality green or spirit purple. If popups never appear, note whether the shop's Recent purchases list shows the buys: the script maps them to portraits through the hero image's `heroid`.
- [ ] **T-8 Vanilla removes**: preset "Vanilla", Apply: `pak77_dir.vpk` is gone (when nothing else on the HUD or Minimap pages is set) and the game is stock.
- [ ] **T-9 After an update**: the Top bar settings survive a game update through the usual "Deadlock updated" re-apply, and `addons verify` is `ok` again.

### 7b2. Minimap page, "Apples & tunnels" card (research/hud/apples-tunnels/NOTES.md)

The dots rebuild `hud_minimap.vxml_c` as a text layout with two includes and add `panorama/scripts/deadtune/apples_tunnels.vjs_c` and `panorama/styles/deadtune/apples_tunnels.vcss_c`. "Clear tunnel switching" alone only appends CSS to the game's `hud_minimap.vcss_c`. Positions are for game build 25712201 (dl_midtown 6722).

- [ ] **A-1 Layout loads**: turn on "Apple spots", Apply, launch through DeadTune (guard on). The game reaches the main menu; in a match the minimap shows as usual, `console.log` has `DeadTune minimap: 36 apple spots, 0 tunnel entrances` and no `FATAL` or line naming `hud_minimap`. `tools\deadtune-cli.exe addons verify` prints `ok`.
- [ ] **A-2 Apple dots in the right places**: in the hideout or a sandbox, walk to two or three apple spawns you know (and wait for a live apple to spawn): each green dot sits on the spot, before an apple appears and after it is eaten, in surface and tunnel view, for any hero. Zoom the minimap in and out (scoreboard open too): dots stay on their spots. Note any dot that is off and by how much.
- [ ] **A-3 Tunnel entrances only for the four heroes**: turn on "Tunnel entrances", Apply. As Rem, Mo & Krill, Rat King and Calico (both forms), purple dots appear on entrances within about 11 % of the map width of you and vanish a little further away without flickering at the edge. As any other hero (try two), no purple dot ever appears. Inside the rat tunnels they hide; leaving brings them back. If no dots appear for an eligible hero, note the hero name the top bar shows in your client language.
- [ ] **A-4 Options take**: dot size 3 and 12 px, a different colour for each kind, radius 5 % and 25 %; each looks as set after Apply and relaunch.
- [ ] **A-5 Clear tunnel switching**: only this switch on, Apply. Entering the rat tunnels: the surface map fades to a faint outline over about 0.2 s and the tunnel layer is brighter; leaving restores the surface. Mid-tunnel (underground) view looks stock.
- [ ] **A-6 Nothing else breaks**: with all three on, hero, ping, objective and camp markers, the zoomed minimap, hover tooltips and minimap clicks behave as before. Then turn on the Top bar extras (spawn timers, purchases) as well: both work together, `addons verify` is `ok`, and `console.log` shows both scripts load.
- [ ] **A-7 Vanilla removes**: Reset all on the card (nothing else set), Apply: `pak77_dir.vpk` is gone and the minimap is stock.

### 7c. Health bar page (experimental)

- [ ] Pick **Big number**, Apply, start a match or the hideout: the health number is clearly bigger and not cut off, max health ("/ 700") is easy to read, the green backer is gone.
- [ ] Take damage to between half and a third of your health: the number turns orange; below that it is red as in vanilla, and the bar and HUD no longer shake.
- [ ] **Hide health regen** hides the small regen number; **Reset** brings the game's health bar back after Apply.
- [ ] Try a hero with an extra bar (Rat King armour, a shield item) and note anything that overlaps.
- [ ] With bytenode's Minimal Healthbar or budhud mod installed alongside, System check lists it under "Other HUD mods".

### 7d. In-game settings page (experimental, docs/plans/ingame-settings/plan.md)

The rows go into the game's own `popup_settings.vxml_c`, rebuilt as a text layout with one include (`panorama/scripts/deadtune/ingame_settings.vjs_c`); nothing of the stock menu is replaced. The Wide FOV slider writes `r_aspectratio` directly; our script copies every change into `citadel_ability_preview_path_debug_draw_dt` (archived, so the game saves it to `cfg\user_convars_*.vcfg`) as 10 plus the ratio, and DeadTune reads that back when it starts.

- [ ] **G-1 Row appears**: In-game settings page, turn on "Wide FOV slider", Apply, launch through DeadTune (guard on). The game reaches the main menu; Settings > Game > Camera Settings shows a "Wide FOV" slider right under the game's FOV slider, 0.010 to 3.200. `console.log` has no `FATAL` and no line naming `popup_settings`. `tools\deadtune-cli.exe addons verify` prints `ok` for `pak77_dir.vpk`. If the menu is stock or the game fails to start, the text layout was refused: copy the console lines and note it.
- [ ] **G-2 Live**: in a match or the hideout, drag the Wide FOV slider: the view widens or narrows while you drag (the menu goes see-through like the stock FOV slider). If nothing changes, open the console and run `r_aspectratio` with no value after dragging; note what it prints. That tells whether the settings control writes a dev-only ConVar at all (plan Phase 0).
- [ ] **G-3 Saved**: set it to 2.400, close the menu, quit the game. `cfg\user_convars_0_slot0.vcfg` holds `"citadel_ability_preview_path_debug_draw_dt" "12.400"`. Start DeadTune: the status line says "Wide view set to ... from the in-game slider", the Overview's Wide view shows that value, `gameinfo.gi` has `r_aspectratio 2.4`, and System check is green. Start the game again: the wide view is already on at the main menu. Change Wide view in DeadTune afterwards (Apply): a DeadTune restart does not put the in-game value back.
- [ ] **G-4 DeadTune group**: turn on a few rows (Shadow quality, Particle cap, Small clutter props), Apply, launch. Settings > Advanced ends with a "DeadTune" subsection (if the title reads `citadel_settings_deadtune` instead, the script's title fix did not take; note it). Drag Shadow quality from 3 to 0 in a match: shadows change live. Toggle Small clutter props: clutter disappears live. Note any row that does nothing live.
- [ ] **G-5 Search**: the settings search box finds "Wide FOV" and "Shadow quality" (optional; our rows carry no search alias).
- [ ] **G-6 Vanilla removes**: Reset all on the page (nothing else set on the HUD pages), Apply: `pak77_dir.vpk` is gone and the settings menu is stock.
- [ ] **G-7 Another settings mod**: with Mixboat's Wide FOV Slider (or any mod replacing `popup_settings`) installed as `pakNN_dir.vpk`, System check warns under "Other settings menu mods"; note which menu the game shows.

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

Each addon is one `game\citadel\addons\pakNN_dir.vpk` that DeadTune writes and removes (its preferred number is 71 to 76; a number already used by another mod is skipped). `deadtune-cli addons list` shows the same table. Four of them are rebuilt from your own `pak01_dir.vpk` on Apply (particle disabler, blur disabler, Sinner light fix, Vindicta scope); no download, and they work offline. Only the soul container is Jayie's hand-made model: a build with the `fetch` feature downloads it with a Download button; otherwise get its `pak01_dir.vpk` from Sqooky's OptimizationLock repository and paste the path into "Import a downloaded file". Nothing on the Mac could prove that the game accepts these paks, so every item here is an in-game check. `tools\deadtune-cli.exe addons verify` reads every installed pak back against your game files; run it after each Apply below and note anything other than `ok`.

- [ ] **A-1 Soul container** (Jayie's file at `pak75`): turn on, download or import, Apply, launch. Does the mod work at `pak75` the way it does at its upstream number? Check setup shows "Performance addons: installed: ...".
The game's scope overlay is a 2048 x 2048 uncompressed texture (16 MiB in VRAM, build 25712201). On the Mac the builder ran end to end on the real header with synthetic pixels (`DEADTUNE_GAME_SNAPSHOT=<snapshot> cargo test -p dt-core --test scope_snapshot`); what the overlay looks like in game, and whether the game loads it, is only provable here.

- [ ] **A-1b Vindicta scope, rebuilt**: turn the addon on (Options: 1080 px, the default), Apply (no download; the card said "Builds from your game files on Apply"). It builds in a moment (one 16 MB read); `game\citadel\addons\pak74_dir.vpk` is about 4.5 MB and `addons verify` prints ok for it. Launch through DeadTune: the status line reads "Testing on this launch: Vindicta scope downscale", then "Deadlock started fine with Vindicta scope downscale". Pick Vindicta, scope in a sandbox: the overlay looks as it does in vanilla and with Tamara's pak89 installed by hand (dark edges fading to the open centre, no banding in the vignette, no halo or jagged edge around the lens, no offset or stretch). Take a screenshot while scoped for the notes.
- [ ] **A-1c Each scope size**: for 720 px and 1440 px in turn: pick it, Apply, `addons verify` ok, launch, scope, screenshot. The pak is about 2.0 MB at 720 and 7.9 MB at 1440. 720 may look slightly soft on a 1440p screen; note whether it is noticeable. The options stop at 1440 px because the game's own overlay is 2048 px: a `side = 2048` (or larger) left in an older profile installs nothing, and Apply removes the pak.
- [ ] **A-1d VRAM and FPS while scoped**: in the same sandbox spot, scoped, read GPU dedicated memory in Task Manager (Performance, GPU) and the FPS counter (`cl_showfps 1`) for vanilla (addon off, game restarted) and for 1080 px. Expected: about 11 MB less VRAM (16 MiB to 4.4 MiB), which may be inside Task Manager's noise; FPS while scoped equal or a little higher, mostly on cards with 4 GB or less. Write down both pairs of numbers, and note any stutter on the first scope-in of a match with each.
- [ ] **A-1e Remove restores the original**: on the scope card press **Remove from game**: `pak74_dir.vpk` is gone, the card is off, and after a restart the scoped overlay is the vanilla one again (same screenshot as vanilla).
- [ ] **A-1f Scope after a game update**: the card reads "Rebuilds from your game files on Apply" and Apply rebuilds it from the new original without a download.
- [ ] **A-2 Blur disabler, rebuilt**: turn on, Apply (no download), launch. `game\citadel\addons\pak72_dir.vpk` holds `panorama/styles/citadel_base_styles.vcss_c` (1324 B, Sqooky's stub) and `panorama/styles/base/citadel_base_styles.vcss_c` (the same size as the game's pak01 entry). In game the minimap frame and the menus have no blur.
- [ ] **A-2b Blur options**: untick "Menus" only, Apply, launch: HUD blur gone, menu blur present; the diagnostic report's read-back line for the stub shows `text 92 bytes` and `source 244412b1`. Untick both: the card says nothing to install and the pak comes off on Apply.
- [ ] **A-3 Blur after an update**: after the next Steam update the card reads "Rebuilds from your game files on Apply"; after Apply the `base/` entry's size equals the new pak01 entry's and `addons verify` is ok. (Before the update, `addons verify` against the new game files reports "not the game's current file" for `base/`, which is the stale copy the rebuild replaces.)
- [ ] **A-3b Sinner's Sacrifice light fix, rebuilt**: in the game console `r_texture_stream_mip_bias 4` (or a DeadTune preset that sets it), walk to a Sinner's Sacrifice vault, note the lights read as grey. Turn the addon on in DeadTune, Apply: the pak builds with no error and `pak73_dir.vpk` holds 2 entries. Launch through DeadTune (guard on); the game must reach the main menu. In a sandbox or lobby look at the vault from near and from across the lane: the light pattern stays sharp black and white at both distances, the vault shows no pop-in between distances, and hitting it still plays the hit animations and orb drops. Optional: `r_texture_stream_mip_bias 0` and 8, pattern identical. Turn the addon off, relaunch: the vault looks stock again (lights blur at bias 4). If the card instead says "Game files unreadable" with a reason mentioning zstd, copy the reason: the game's model block changed compression and the builder refused it.
- [ ] **A-4 Particle disabler, all groups**: turn on with everything hidden (the default), Apply. The pak at `pak71` has 108 `.vpcf_c` entries and no `.vtex_c`, and no download happened. Launch, take damage to low health: no red screen vignette. Walk through vent steam: no screen steam overlay. Compare with upstream `pak02` installed by hand: identical behaviour (it is the same file, the game's own `particles/empty.vpcf_c`).
- [ ] **A-5 Particle disabler, some groups**: in Options untick "Low health vignette" and "Damage and death flashes", Apply. The vignette is back; hero debuff effects (for example Infernus burn) and vent steam stay hidden. The pak holds only the hidden paths. Tick every group back to visible: the pak is removed.
- [ ] **A-6 Hero names in the particle list**: the labels with a codename in brackets (bookworm, butcher, druid, familiar, fencer, frank, hijack, priest, punkgoat, synth) and the guessed ones (Lady Geist for `ghost`, Fathom, Doorman, Wrecker, Paradox's `chrono_sphere`) were not verified. Trigger each effect in the sandbox and write the right hero next to the codename; the table is `crates/dt-core/src/addons/particles.rs`.
- [ ] **A-7 Slot collision**: with QoL Lite (`pak01`) or another mod at `pak71` present, enabling the particle disabler lands at the next free number (`pak70`) and Apply never touches the other mod's file. Disabling removes only ours.
- [ ] **A-11 World clutter remover**: turn it on (City ambience is pre-ticked), Apply, launch. Street steam, chimney smoke and the crows around the towers are gone; nothing else changed. Then tick Graves effects and Walker effects one at a time and check against a Graves and the Walker in the sandbox: their attacks must still be readable. With Graves ticked, the wall marker shows only with `sc_fade_distance_scale_override` at 4 or lower (the card offers a button). Note any FPS difference near vents.
- [ ] **A-12 Everything (offline only)**: tick Everything, Apply, launch into the sandbox, not a match. Almost no particles draw. Untick and Apply: they are back. The pak is `pak78`.
- [ ] **A-8 Ranked-safe**: with addons installed, Ranked-safe mode removes every DeadTune pak (HUD too), leaves other mods, and queueing matchmaking still works (**P0-3**). Turning it off brings them back on the next Apply.
- [ ] **A-9 Foreign file**: overwrite one of our paks with any other file. Check setup says "not what DeadTune wrote"; Apply reinstalls at a new number and never deletes the replaced file.
- [ ] **A-10 Texture downscaler from the GUI**: same checks as section 10, started from the Addons card (Build button, progress with Cancel, stats line after). Cancel mid-way leaves no `pak76` files behind. The pak number is 76 here (the example in section 10 uses 78).
- [ ] `--quarter --install` repeats the above with a stronger effect.
- [ ] Delete the `pak78*` files, relaunch: full quality is back.
- [ ] A multi-file output (`pak78_dir.vpk` plus `pak78_000.vpk`) mounts. Half size on a full game should produce one.
- [ ] Matchmaking still queues with the addon mounted (same question as **H0-4**).

- [ ] Links open: an author credit opens the browser, and System check's "Open the Windows setting" opens the right Settings page (`ms-settings:` links).

### 12. Launch guard (addons and the HUD tested on the next launch)

DeadTune remembers the set of its paks (performance addons and the HUD's `pak77_dir.vpk`, each with its sha256) that Deadlock last started with (`guard.toml` in the data folder). When the game process appears, every pak that is new or rebuilt since then is on trial: the trial passes once the console log shows `DEADTUNE_BOOT` or the game stays up for 90 s, and fails when the game exits within 60 s or the log shows `FATAL ERROR`. On a failure DeadTune deletes exactly the addon paks that changed (never another mod's file, never `gameinfo.gi`), switches them off in the profile and shows a red banner. A HUD pak that was on trial goes back to the last HUD pak the game started with (DeadTune keeps one copy, `hud-verified\` in the data folder, for the same game build), or is removed when there is none; the HUD settings stay as they are, and Apply leaves that exact HUD build out until you change a HUD setting or press Try again.

- [ ] **G-1 Badges**: Addons page shows "Not tried in game yet" on a freshly installed addon, "Verified in game" (green) after one good launch, and the top note "New addons are tested on the next launch".
- [ ] **G-2 Verified by boot marker**: launch from DeadTune with a new addon; the badge turns green within a few seconds of the main menu (the `DEADTUNE_BOOT` line), not only after 90 s.
- [ ] **G-3 Verified from Steam**: start the game from Steam instead (with `+exec deadtune_boot -condebug` in the launch options, or without: then the 90 s rule applies). Same badge.
- [ ] **G-4 Failure**: break the start on purpose with a pak that DeadTune owns (the old experimental blur generator no longer exists; if no addon fails the start on its own, run `deadtune-cli hud apply` with a layout, then corrupt a few bytes inside its `pak77_dir.vpk`, or report that you found no way to make a start fail). Apply, launch. Expected: the FATAL dialog or an early exit, then the banner "Deadlock didn't start with <addon name>. DeadTune removed them, so the game will start normally now." with the FATAL line, the pak gone from `game\citadel\addons`, the addon switched off, and the badge "Broke game start, removed" (hover shows date and the line). "Launch Deadlock again" starts the game, which should now come up. If the dialog keeps the process alive and no FATAL line reaches `console.log`, the trial wrongly passes after 90 s: write that down, it means the log location is wrong (section 3).
- [ ] **G-5 Several suspects**: turn on two new addons at once and break the start: the banner lists both; "Enable one at a time" switches the first back on and installs it; launch, repeat. The culprit ends with the red badge, the other with the green one, and the status bar narrates each step.
- [ ] **G-6 Restart mid-trial**: close DeadTune right after launching with a new addon, reopen it while the game runs: the trial resumes (the status says "Testing on this launch") and passes.
- [ ] **G-7 Safe mode**: the small menu beside Launch Deadlock > "Safe mode: launch without addons" removes every DeadTune pak (HUD too), other mods stay, the game starts clean; the button reads "Launch (safe mode)" until "Restore addons" from the same menu puts them back.
- [ ] **G-9 HUD on trial**: change any HUD setting (for example the minimap's opacity on the HUD page), Apply, launch through DeadTune. The status line reads "Testing on this launch: DeadTune's HUD changes", then "Deadlock started fine with DeadTune's HUD changes"; the HUD pages show "Deadlock started fine with these HUD changes on <date>"; `tools\deadtune-cli.exe addons guard` prints `launch guard: DeadTune's HUD changes: verified: ...` (the CLI and the GUI read the same records); `%APPDATA%\DeadTune\backups\hud-verified\` holds one `<sha>.vpk` and its `<sha>.toml`. Launch again without changing anything: no "Testing" line.
- [ ] **G-10 HUD rolled back to the last good one**: after G-9, change another HUD setting (for example turn on a top bar extra), Apply, then break the start on purpose. Two ways, both exercise the real path: quit the game from Task Manager within 60 s of it appearing (early exit), or, while it is still loading, run `Add-Content "<Deadlock>\game\citadel\console.log" "FATAL ERROR: test"` in PowerShell (a FATAL line). If you found a HUD change that really stops the game, use that and copy the FATAL line into the notes. Expected: the red banner "DeadTune's HUD changes stopped the game from starting, so DeadTune put back the last HUD that worked. Your settings are kept; try again or turn off the newest HUD change.", Show details lists "HUD parts in the version that failed: ..." (here: layout, top bar), and `pak77_dir.vpk` is byte-identical to the copy in `hud-verified\` (compare with `Get-FileHash`). The HUD pages say the changes are being left out, and an Apply of any other setting does not rewrite `pak77_dir.vpk`. The next launch is not a trial and the game shows the earlier HUD.
- [ ] **G-11 HUD removed when nothing worked before**: delete `%APPDATA%\DeadTune\backups\hud-verified\`, change a HUD setting, Apply, break the start as in G-10. The banner reads "DeadTune's HUD changes stopped the game from starting, so they were turned off." and `pak77_dir.vpk` is gone. Press "Try the HUD again": the pak is back and the next launch tests it again. System check shows a "HUD launch test" row that warns while the failure stands.
- [ ] **G-12 Addon and HUD together**: turn on a new addon and change a HUD setting, Apply, break the start. The banner names both ("Deadlock didn't start with <addon> and DeadTune's HUD changes."), both paks come out, "Enable one at a time" brings the addon back first and the HUD on the launch after.
- [ ] **G-13 Safe mode with a HUD**: with a HUD installed, safe mode takes `pak77_dir.vpk` out too and a launch in safe mode is not a trial; "Restore addons" puts the same HUD back. A launch in safe mode forgets what was verified, as it does for addons, so the next normal launch tests the HUD again and should pass.
- [ ] **G-8 Read-back**: `tools\deadtune-cli.exe addons verify` prints "ok" for every installed pak. Overwrite the last bytes of one pak with a hex editor: `addons verify` names the entry with "crc mismatch" and exits 1.

### 13. SideLock (downloaded preset)

SideLock is CC BY-NC-ND, so DeadTune never ships it; it downloads the author's file from GameBanana and checks it against a pinned sha256.

- [ ] **S-1 Download**: Overview > All presets > SideLock. The dropdown shows "by hitmeupwhenyourelonely, CC BY-NC-ND 4.0, downloaded from GameBanana"; the card says "SideLock isn't downloaded yet". Press **Download from GameBanana**: the status says "SideLock is ready" and Apply works. `%APPDATA%\DeadTune\presets\sidelock\gameinfo.gi` exists.
- [ ] **S-2 Only settings**: after Apply, open `game\citadel\gameinfo.gi`: the ConVars block changed, but `SceneSystem`, `RenderSystem` and `FileSystem` are byte-for-byte what they were (compare with the original in DeadTune's backups). The outline and visibility settings it sets are refused (`deadtune-cli diff` lists them).
- [ ] **S-3 Import offline**: delete the `presets\sidelock` folder, download `cfg.zip` from https://gamebanana.com/mods/722944 in a browser, paste its path into the Import box: same result as S-1. Importing a random file says it is not a game settings file.
- [ ] **S-4 Review**: put any other `gameinfo.gi` (say a copy of your game's) into the Import box: the card says the file differs, "Show the N changed settings" lists them, nothing changes until **Accept**; **Discard** drops it.
- [ ] **S-5 CLI**: `tools\deadtune-cli.exe presets` shows the licence and state; `presets fetch sidelock`, then `profile new side.toml --base sidelock` and `diff --profile side.toml` work.

### 14. Practice mode (Performance page)

Since the September 2026 update the game ignores the big shadow and fog ConVars in `gameinfo.gi`. Practice mode edits the `SceneSystem` section instead, the way the SideLock config (GameBanana 722944) does: shadows (`CSMCascadeResolution`, the shadow atlas and texture sizes, `DynamicShadowResolution` to 0), fog (`VolumetricFog`, `CubemapFog`, `NonTexturedGradientFog` to 0) and batching (`LayerBatchThresholdFullsort 20`, `DisableLateAllocatedTransformBuffer 1`, `MinimumLateAllocatedVertexCacheBufferSizeMB 64`). The game's matchmaking check refuses "unsupported changes" to `Engine2`, `MaterialSystem2`, `NetworkSystem`, `Particles`, `RenderSystem`, `SceneSystem` and `WorldRenderer`, so we expect queueing to be refused while it is on. Nothing here could be proven on the Mac. DeadTune only ever touches keys it wrote itself: before it writes a practice value it records what the key held (`practice.toml` in its data folder), and switching the group off puts that back. Keys it never wrote are left exactly as they are, so a hand-installed SideLock `SceneSystem` survives a normal Apply. Ranked-safe mode is the explicit "make me queueable" action and resets all of them to Valve's stock values, whoever wrote them; leaving Ranked-safe returns to the profile's state.

Setup: a `gameinfo.gi` that is otherwise stock (Vanilla preset or Ranked-safe on), the FPS counter on (`cl_showfps 1` in the console, or Steam's overlay), and the same sandbox spot for every measurement. Note the FPS at that spot before you start.

- [ ] **PM-1 Writes**: Performance > Practice mode, turn on all three, Apply. The action bar says the changes take effect next launch, the sidebar shows "Practice mode on", and Overview > Status shows the same with a Performance button. In `game\citadel\gameinfo.gi` the `SceneSystem` block has the eleven zeroed values and the three batching lines just before its closing brace; nothing else in the file changed, and the file still uses CRLF.
- [ ] **PM-2 Loads**: launch, open the sandbox. Are shadows gone and fog gone? Write down the FPS at the reference spot with all three on, then with only shadows, only fog, only batching (Apply and relaunch each time). Note anything odd (black areas, missing lighting, crashes at map load).
- [ ] **PM-3 System check**: with practice mode on, the "Matchmaking sections" row is a Warning saying practice mode is on and matchmaking may refuse to queue. Copy the diagnostic report once in this state.
- [ ] **PM-4 Queueing refused?** (critical): with practice mode on, go to Play and queue for an unranked match, then a ranked one. Does the game refuse with an "unsupported changes" or similar message, or does it queue? Copy the exact wording. Then try a bot match and the sandbox: those should work.
- [ ] **PM-5 Ranked-safe restores queueing** (critical): leave practice mode's switches on, turn on Ranked-safe mode (Safety & setup), relaunch. The `SceneSystem` block is back to stock (`CSMCascadeResolution 2048`, `VolumetricFog 1`, no `LayerBatchThresholdFullsort` line) and the "Matchmaking sections" row is OK. Can you queue now? Turn Ranked-safe off, relaunch: practice mode is back without touching the switches.
- [ ] **PM-6 Off restores what was there**: turn the three switches off, Apply. `gameinfo.gi` is byte for byte what it was before PM-1 (compare with the backup in Advanced > Backups, or `fc /b`), and `practice.toml` is gone from the data folder. Queueing works.
- [ ] **PM-7 The record survives a restart**: turn shadows on, Apply, close DeadTune. `practice.toml` in the data folder lists the shadow keys with their old values. Reopen DeadTune, switch shadows off, Apply: the old values are back.
- [ ] **PM-8 SideLock stays yours**: with the SideLock `gameinfo.gi` installed by hand and practice mode off, press Apply on any preset. The `SceneSystem` block is untouched (its zeros and the three batching lines are still there). System check's "Matchmaking sections" row lists those keys plus SideLock's other edits (for example `RenderSystem/SwapChainSampleableDepth`), says Ranked-safe mode resets the shadow, fog and batching keys and points to Verify integrity for the rest. Turn practice mode on and off again: the block is still SideLock's, because DeadTune never changed those values. Does queueing fail in that state too?
- [ ] **PM-9 CLI**: `tools\deadtune-cli.exe practice on --fog --profile my.toml`, then `diff --profile my.toml` lists the three fog keys under "practice mode", `apply --profile my.toml --yes` writes them, `ranked-safe --yes` restores stock, `practice off --profile my.toml` drops the table from the profile.

### 15. System check page

- [ ] The page opens with a summary ("Everything looks good" or "N things need your attention"), then a card per problem with "What to do", then every check grouped as Deadlock files, HUD and addons, Windows and hardware, DeadTune. "Check again" reruns; "Copy report" copies the diagnostic report.
- [ ] **Steam launch options**: in Steam set Deadlock's launch options to `+fps_max 60`, press Check again: a warning names `+fps_max 60`. With only `+exec deadtune_boot` (or `-dx11`) it passes and shows the options. Clear them afterwards.
- [ ] **Mods from other tools**: copy any mod pak into `game\citadel\addons` as `pak60_dir.vpk`: the row lists it; remove it again.
- [ ] **Free disk space**: shows the free space on the game's drive; it warns under 10 GB (check the number against Explorer).
- [ ] **Steam background work**: while Steam shows "Processing Vulkan shaders" (or `fossilize_replay.exe` runs in Task Manager) it warns; otherwise it passes.

### 16. Send the diagnostic report

Whenever something misbehaves, and once when everything works, press **Copy report** on the System check page (or run `tools\deadtune-cli.exe doctor --report > report.txt`) and send the text along with your notes. It holds the DeadTune version, the `SearchPaths` block of `gameinfo.gi`, the files in `game\citadel\addons` with sizes and who owns each, DeadTune's addon and HUD records, the launch guard state, the last launch arguments, the last 80 lines of `console.log` and the read-back of every installed pak. It contains no account data; paths show your Windows user name.

### 17. Launch options and Vulkan

The menu beside **Launch Deadlock** > **Launch options** (or the Launch tab in the advanced view) holds the renderer choice, Skip intro video, the console window, extra options, and a check of every option against the game's own list (`research/configs/OptimizationLock/launch_options.txt`).

- [ ] **L-1 Migration**: an existing install keeps its old launch options. Before updating, note what was in Advanced > Launch; after, the same options show up as toggles plus extras, nothing missing.
- [ ] **L-2 Check**: paste `-vulkan -novid -nosplash -high -noborder -novsync -dx11 -threads 10 -noaaf -noshadows -nod3d9ex -disableframecap` into Extra options. `-nosplash`, `-noaaf`, `-noshadows` and `-disableframecap` show amber "not in this game build"; `-vulkan` and `-dx11` show red "conflicts with" each other. `tools\deadtune-cli.exe launch-options check "<same text>"` prints the same and exits 1.
- [ ] **L-3 Copy for Steam**: the button copies the full line, ending in `+exec deadtune_boot -condebug`. Paste it into Steam (right-click Deadlock > Properties > General > Launch Options), start the game from Steam, and confirm Instant changes still tick (section 3). Then clear Steam's box again.
- [ ] **L-4 Vulkan starts and renders**: renderer Vulkan, nothing else changed, Launch Deadlock. The game reaches the main menu and a bot match renders normally (no black screen, missing shadows or flicker). Check it really is Vulkan: search `console.log` for "Vulkan" and write down the line that names the render API.
- [ ] **L-5 Shader cache**: the first Vulkan start compiles shaders, so expect long loading and stutter in the first match or two. Note how long the first start took to the main menu and whether the second start is faster. Don't judge Vulkan FPS on the first match.
- [ ] **L-6 FPS vs default**: on the same map and spot (a bot match, measured the way section 8 does), compare average and 1% low FPS for Default and Vulkan, after Vulkan's shader cache is warm. Note GPU model and driver version with the numbers.

### 18. Developer checks (repo checkout on Windows)
- [ ] `cargo test --workspace --all-features` passes, including the Windows-only `push_uses_crlf_on_windows`.
- [ ] With `core.autocrlf=true`, a fresh clone still passes the catalog drift test.
- [ ] `cargo test -p dt-core --features fetch -- --ignored fetch` downloads every preset, SideLock included, and SideLock still matches its pinned sha256.
- [ ] `tools\deadtune-cli.exe doctor` matches the GUI's Check setup; its y/N confirm prompt works in cmd and PowerShell.
- [ ] `cargo test -p dt-core --features fetch -- --ignored fetch_latest` downloads every preset.
- [ ] `tools\deadtune-cli.exe doctor` matches the GUI's System check; its y/N confirm prompt works in cmd and PowerShell.

### 19. Game files page (snapshots of the game's interface files)

The Game files page (sidebar, under More) copies the HUD, settings menu and main menu layouts, stylesheets and scripts out of `pak01_dir.vpk`, plus `gameinfo.gi`, the cfg files and the Steam build id, into `%APPDATA%\DeadTune\game-files\<build>-<date>\`: `raw\` holds the files as the game has them, `text\` the decoded CSS, XML and JavaScript, `manifest.toml` every file's size and CRC, `pak01.tsv` the whole archive's file list. "Compare with previous" writes `diff-<old>-to-<new>.md` into the newer folder. Nothing in the game folder is written. The Mac could only run this on a fake install (16 files in 0.4 s); the real numbers come from here.

- [ ] **GF-1 Take a snapshot**: open Game files. The header shows the current build id, and every category shows a file count and size within a second of opening the page. Press **Take snapshot** with the default choices (everything ticked, decode on, 8 MB cap). Note the time the progress bar runs and the green line afterwards (files, MB, seconds). Write down the folder size in Explorer and how many files `text\` holds.
- [ ] **GF-2 Decoded settings menu**: **Open snapshot folder**, go into the newest folder, open `text\panorama\layout\popups\popup_settings.xml` in a text editor. It should be readable XML starting with `<root>` and contain a `CitadelSettingsSlider` row for the camera FOV. If instead there is a `popup_settings.vxml_c.strings.txt`, copy its first line (the decode error) into the notes; the raw file is still in `raw\`, send it along.
- [ ] **GF-3 Stylesheets and scripts**: `text\panorama\styles\hud.css` is CSS, `text\panorama\scripts\` holds `.js` files that read as JavaScript. Count the `.strings.txt` files under `text\`; ideally none.
- [ ] **GF-4 Big files**: in `manifest.toml` the scope texture (`panorama/images/hud/crosshair/scope_common_psd.vtex_c`, 16 MB, over the 8 MB cap) has `stored = "full"` and the whole file is under `raw\`: every file a DeadTune feature builds from is stored whatever the cap. The Sinner's Sacrifice mask and model have `stored = "full"` too. A snapshot of build 25712201 taken with DeadTune 0.9.0 has only a `.header` for the scope; taking that build again replaces it with the full file.
- [ ] **GF-5 Same build again**: press **Take snapshot** again without a game update. It finishes in a moment, the status says 0 written and everything "already there", and the snapshots list still has one entry for this build.
- [ ] **GF-6 After the next game update**: with "Snapshot automatically after game updates" on, leave DeadTune open while Steam updates Deadlock. After the update the status line reads "Deadlock updated. Snapshot saved ..." and the page shows a "Latest comparison" card: build A to B, counts per category, and the amber "Check these first" list naming DeadTune features. Press **Copy summary** and paste the text into the notes. Then **Open report** and check the per-file diffs look like real CSS and XML changes. The "Elsewhere in the game" line should count models, particles and maps the update touched.
- [ ] **GF-7 Compare by hand**: pick the two snapshots in the pickers (older left, newer right) and press **Compare with previous**; the report is the same as GF-6. Swap the order and compare again: added and removed swap.
- [ ] **GF-8 Cancel and delete**: start a snapshot and press **Cancel** mid-way (easiest right after a game update, when nothing is reused): the status says "Snapshot cancelled" and no half folder is left in `game-files\`. **Delete** on a snapshot asks once ("Really delete?") and removes the folder.
- [ ] **GF-9 CLI**: `tools\deadtune-cli.exe snapshot list` shows the same snapshots; `snapshot take --categories hud,config` writes only the HUD and settings files into the current build's folder; `snapshot diff previous latest` prints the summary and the report path.
- [ ] **GF-10 UI images**: `tools\deadtune-cli.exe snapshot take --images all` copies every `panorama\images` file (about 3.3 GB, 2706 files) into the current build's folder and writes `text\panorama\images\...\*.png` and `*.svg` next to the layouts. Note the time and the folder size. The command ends by listing every file it could not decode; ideally that list is empty, otherwise paste it into the notes. Open a few PNGs in Explorer (`text\panorama\images\minimap\base\minimap_midtown_mid_psd.png` should be the map, `hud\top_bar\chat_texture_png.png` the chat backer) and an SVG (`hud\top_bar\icon_ultimate.svg` in a browser). Then, from a repo checkout, run the decode census over the folder and paste its output (the format distribution and the SVG feature survey) into the notes:

  ```powershell
  $env:DEADTUNE_GAME_SAMPLES = "$env:APPDATA\DeadTune\game-files\<build>-<date>"
  cargo test -p dt-core --lib --features svg -- texture --nocapture
  ```

  Both `game_samples` tests must pass. Also try `tools\deadtune-cli.exe texture png "<folder>\raw\panorama\images\hud\crosshair\scope_common_psd.vtex_c" scope.png` and look at `scope.png` (the 4096 scope vignette).

### 20. UI image overrides from the CLI (the GUI page is section 21)

DeadTune can put your own PNG or SVG in place of any interface image under `panorama/images/` (minimap icons, top bar, portraits, item icons). The images ship inside the HUD addon (`addons\pak77_dir.vpk`), re-encoded from the game's own file on every apply. A PNG becomes an uncompressed BGRA texture at the game image's size (letterboxed with transparency) or at its own size with `--fit own`. On the Mac this was only checked against a fake install; whether the engine draws the result is the open question. The HUD addon is covered by the launch guard (section 12): if the game fails to start with new images, DeadTune puts back the last HUD that worked or removes it. Without the GUI running, `tools\deadtune-cli.exe hud remove --yes` still takes it out by hand.

Make two test images first, in any paint program: `C:\dt\test.png`, 128x128, solid **orange** (255, 128, 0) with a black X, and `C:\dt\test.svg` containing `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#ff8000"/></svg>`. Orange tells us the channel order is right: if an icon comes out blue, red and blue are swapped.

- [ ] **IC-1 Pick paths**: take a snapshot (section 19), open `pak01.tsv` in the newest snapshot folder and search for `panorama/images/hud/minimap` and `top_bar`. Write down one minimap hero or objective icon ending in `.vtex_c`, one top bar icon ending in `.vtex_c`, and one top bar or `hud/icons` image ending in `.vsvg_c` (`panorama/images/hud/icons/rejuvenator.vsvg_c` is the Rejuvenator icon the top bar shows).
- [ ] **IC-2 Set**: `tools\deadtune-cli.exe hud icon set <minimap path> C:\dt\test.png --layout C:\dt\icons.toml`, the same for the top bar `.vtex_c`, then `hud icon list --layout C:\dt\icons.toml`. Both are listed as `png, fit original`; the PNG copies are in `%APPDATA%\DeadTune\backups\icons\`. Delete `C:\dt\test.png` and keep going: DeadTune uses its copy.
- [ ] **IC-3 Ship and look**: `hud apply --layout C:\dt\icons.toml --yes`, then `addons verify` (the HUD layout line says ok). Launch, open a sandbox or hideout match: the minimap icon and the top bar icon are orange with the X, not stretched, transparent around the edges. Note anything odd (blank square, black box, wrong colour, wrong size, crash). Screenshot it.
- [ ] **IC-4 Own size**: make a 512x512 orange PNG, `hud icon set <minimap path> big.png --fit own --layout C:\dt\icons.toml`, apply, launch. The icon should draw at the same on-screen size as before, sharper. If it draws bigger or cropped instead, note it: the GUI would then default to the game's size only.
- [ ] **IC-5 SVG for a vector icon**: `hud icon set <.vsvg_c path> C:\dt\test.svg --layout C:\dt\icons.toml`, apply, launch: the icon is an orange circle. If `hud apply` prints `HUD icon left out: ... does not hold SVG text where expected`, our guess at the `.vsvg_c` layout is wrong: send that line and the file from the snapshot's `raw\` folder.
- [ ] **IC-6 Experimental PNG in a vector icon**: `hud icon set <.vsvg_c path> C:\dt\test.png --layout C:\dt\icons.toml` (the list says `png in svg (experimental)`), apply, launch. Write down exactly what shows: the orange PNG, an empty space, or the game's own icon. This decides whether the GUI offers PNGs for vector icons or SVG only.
- [ ] **IC-7 Reset**: `hud icon reset <minimap path> --layout C:\dt\icons.toml`, apply, launch: the minimap icon is the game's again, the others are still yours. `hud icon reset-all --layout C:\dt\icons.toml` and apply: with nothing else in the layout, `addons\pak77_dir.vpk` is removed. `ranked-safe --yes` also removes the HUD addon with any icons in it.
- [ ] **IC-8 After a game update**: with overrides installed, let Steam update Deadlock. `hud status` says the addon is for the old build; `hud apply --layout C:\dt\icons.toml --yes` rebuilds every icon from the updated game files and they still show in game. If Valve renamed or removed one of the images, apply prints `HUD icon left out: <path>: the game no longer has this image ...` and the other icons still ship.

### 21. UI images page (simple view: UI images; advanced view: HUD, UI images)

The same overrides as section 20, from the window. The page lists every image in your `pak01_dir.vpk` under `panorama/images/` (about 2706), decodes only the tiles on screen, and lets you replace one by dropping a file on it. Changes are pending until **Apply**, like every other HUD edit. Use the test images from section 20 (`C:\dt\test.png` orange with a black X, `C:\dt\test.svg` an orange circle). On the Mac this was only seen on the fake install and on a 2706-file copy of it; the real images and the in-game result come from here.

- [ ] **UI-1 Browse**: open **UI images** in the sidebar. The header says "From your game files" and about 2706 images. Note how long the page takes to show the first tiles, then scroll the whole grid fast with the mouse wheel: it should not stutter, and tiles fill in within a moment of stopping. Click the folder chips (minimap, hud, heroes, items, upgrades, icons) and a subfolder under hud (top_bar). Type `ultimate` in the search box. Tick **Changed by me** (empty for now). Write down any tile showing a red "Can't show": hover it and copy the reason into the notes, with its path.
- [ ] **UI-2 Preview**: click a minimap icon and a top bar `SVG` tile. The right panel shows it large on a checkerboard with its size and format; vector icons are drawn as pictures, not code. Try **1:1**. **Copy** puts the full game path on the clipboard.
- [ ] **UI-3 Export**: on a texture press **Export PNG**, on a vector icon **Export PNG** and **Export SVG**, then **Open folder**: Explorer opens `%APPDATA%\DeadTune\exports\` with the files under their game folders. Open them: the PNG matches the preview, the SVG opens in a browser.
- [ ] **UI-4 Swap a minimap icon (PNG)**: select a minimap `.vtex_c` icon, keep **Fit to original size**, and drag `C:\dt\test.png` from Explorer onto the window. A green line says "Replaced"; the panel shows the game's image and yours side by side; the tile shows yours with an amber dot; the sidebar counts 1 next to UI images; the Apply bar is lit. Also drop the PNG straight onto a different tile without selecting it first: that tile is the one replaced.
- [ ] **UI-5 Swap top bar icons (PNG and SVG)**: drop `C:\dt\test.png` on a top bar `.vtex_c` icon, and `C:\dt\test.svg` on a top bar or `hud/icons` `SVG` tile (for example `hud/icons/rejuvenator`). Then paste `C:\dt\test.svg` into the **Replace** box of another vector icon and press **Replace**. Drop the PNG on a vector icon too: it is accepted with an amber "test" note (IC-6 decides whether it shows).
- [ ] **UI-6 Bad files**: drop a `.txt` or `.jpg` on a tile: one red sentence says it isn't a PNG or SVG, nothing changes. Drop the SVG on a `.vtex_c` picture: the sentence says it needs a PNG.
- [ ] **UI-7 Apply and look**: press **Apply**, launch, open a sandbox or hideout match. The minimap icon and both top bar icons are yours (orange, not stretched, transparent around the edges). Screenshot it. Back in DeadTune, any replacement that could not be built shows a red dot on its tile and a red sentence in the panel; copy it into the notes.
- [ ] **UI-8 Own size**: on the minimap icon pick **Keep my image's size**, Apply, launch: compare with UI-7 (see IC-4).
- [ ] **UI-9 Reset**: **Reset** on the minimap icon, Apply, launch: the game's icon is back, the others are still yours. **Reset all images**, Apply: the images are gone (and with no other HUD edits, `addons\pak77_dir.vpk` is removed).

### 22. Colour edits on the UI images page (Edit colours)

Any image can be recoloured without a file of your own: the right panel's **Edit colours** section tints, colorizes, overlays, shifts hue, saturation, brightness, contrast and opacity, or inverts the game's own picture; vector icons also list their colours so each one can be swapped. DeadTune rebuilds every edited image from your game files at Apply (the profile stores only the recipe), so a game update that changes an icon is picked up on the next Apply. Edits are pending like every other HUD change: the Apply bar lights up, **Discard changes** throws them away, and Ctrl+Z / Ctrl+Shift+Z (or the Undo / Redo buttons on the page) step back through this session's edits before Apply. On the Mac this was only seen on the fake install's stand-in images; the real icons and the in-game result come from here.

- [ ] **CE-1 Tint a minimap icon**: open **UI images**, click the **minimap** folder, select a hero or objective `.vtex_c` icon. In **Edit colours** pick **Tint**, press the red swatch, drag **Strength** to about 80: the "Yours" preview turns red next to the game's; the edit appears in the list below the sliders; the tile shows the tinted picture with an amber dot. Press **Apply**, launch, open a sandbox or hideout match: the icon is red on the minimap, not stretched, transparent around the edges. Screenshot it.
- [ ] **CE-2 Tint the whole minimap folder**: back in DeadTune, with the **minimap** folder open and nothing selected, press **Apply to folder** after choosing **Tint** and a colour (or select one icon, set the tint, then press **Apply to folder**): every tile in the folder shows the tint, **Changed by me** counts them all. Ctrl-click three tiles and press **Apply to 3 selected** with a different colour: only those three change. **Apply**, launch: the whole minimap carries the tint.
- [ ] **CE-3 Recolour an SVG top bar icon**: open **hud / top_bar** (or **hud / icons**), select an `SVG` tile such as `hud/icons/rejuvenator`. **Edit colours** lists the icon's colours as swatches; press the picker next to the white one and choose the Sapphire swatch (`#4d75c3`): the preview recolours only that part of the icon. Also try **Colorize** with the Amber swatch on a white icon: it becomes amber at the same lightness. **Apply**, launch: the top bar icon shows the new colours, still crisp at any UI scale (it stays a vector icon). Note any icon that comes out blank or black: copy its path.
- [ ] **CE-4 Undo and discard**: make three edits on three different images, press Ctrl+Z twice: the last two edits are gone from the tiles and the panel; Ctrl+Shift+Z brings one back. Press **Discard changes** in the Apply bar: every unapplied edit disappears and Undo is greyed out.
- [ ] **CE-5 Reset the folder, then everything**: with the minimap folder tinted (CE-2) and applied, press **Reset this folder**, **Apply**, launch: the minimap is vanilla again while the top bar icon from CE-3 is still yours. Then **Reset all images**, **Apply**: with no other HUD edits, `addons\pak77_dir.vpk` is removed from the game folder, and the game shows nothing of DeadTune's.
- [ ] **CE-6 Edits survive a game update**: with a tinted icon applied, let Steam update Deadlock, press **Apply** again: the tint is rebuilt from the new icon (the panel says nothing is left out). If Valve removed or renamed the image, the tile shows a red dot and the panel one red sentence, and the other edits still ship.
