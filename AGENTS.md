# AGENTS.md

Guidance for coding agents working on DeadTune. Read this before changing anything. Product spec: `PLAN.md`. User docs: `README.md`. Release process: `docs/releasing.md`. Windows test checklist: `docs/testing-windows.md`. HUD design: `docs/plan-hud.md`.

## What this is

DeadTune is a Windows desktop app (Rust, egui/eframe 0.36) that tunes Deadlock's performance settings: the `ConVars` block of `game/citadel/gameinfo.gi`, `cfg/video.txt`, HUD layout and colours, and performance addons. It works only through files, launch options and the official console. No DLL injection, memory access or hooks. Competitive-information ConVars (outlines through walls, visibility) are on a denylist and never changed. HUD features that re-present information through the game's own UI (top bar dimming, spawn timers, urn soul lead, purchase popups) are allowed: Simon decided on 2026-10-05 that top bar information features are safe. They stay opt-in and off in the Vanilla preset.

The game runs on Windows (primary) and Linux/Proton. Development happens on macOS, which cannot run the game. Anything that touches the real game is proven on Windows by Simon, using `docs/testing-windows.md`.

## Layout

| Path | Contents |
|---|---|
| `crates/dt-core` | All logic, no UI, fully unit-tested: `gi` (lossless ConVars editor), `video`, `catalog`, `preset`, `profile`, `apply` (pure plan, then effects), `backup`, `locate`, `watch`, `launch`, `power`, `bridge/` (exec-file, netcon, clipboard, console ack and log tail, boot cfg), `bench`, `doctor`, `hud/` (VPK reader/writer, `art` (game images the HUD previews draw), compiled resources, Panorama CSS, layout, SearchPaths, minimap colours, `ingame` rows in the game's settings menu, UI image overrides), `usercfg` (the game's saved player settings), `addons/` (registry, install, verify, launch guard, generators), `texture/` (vtex parser, mip stripping, PNG/SVG encoding into game images, `adjust` colour operations and `svg` palette edits for image overrides), `update/` (signed self-update), `winfps/` (read-only Windows FPS checks), `snapshot/` (game file snapshots and diffs, `docs/plans/game-files/plan.md`; `snapshot::images` exports every UI image with `manifest.json`), `zip` (read and write) |
| `crates/dt-gui` | eframe app, binary `deadtune`. `state.rs` holds `AppState` and every transition, with tests. Views are thin: `simple.rs` (sidebar view), `advanced.rs`/`views.rs`, `compact.rs` (mini window), `hud_view.rs`, `minimap_view.rs` (previews draw the game's images through `hud_art.rs`), `addons_view.rs`, `game_files_view.rs` (with `snapshots.rs` for its job), `images_view.rs` (UI images page; state in `images.rs`, colour edits, bulk, undo and multi-select in `images_edit.rs` with their panel in `images_edit_view.rs`, thumbnail workers and cache in `thumbs.rs`, "Save all images" job in `images_export.rs` and its strip in `images_export_view.rs`), `update_view.rs`, `live_status.rs`, `theme.rs` |
| `crates/dt-cli` | Binary `deadtune-cli` over the same core: `doctor`, `diff`, `apply`, `ranked-safe`, `restore`, `push`, `launch`, `hud`, `addons`, `bench`, `snapshot`, `self-update`, etc. Hand-rolled arg parsing |
| `catalog/` | `catalog.toml` is generated from `research/data/convar_catalog.csv` plus `curated.toml`. Never edit it by hand: run `cargo run -p dt-core --example gen_catalog`. A test fails on drift |
| `research/` | Upstream presets (GPL-3.0), mod VPKs, convar dumps, reference code, HUD research. Golden fixtures for tests |
| `scripts/` | `fake-install.sh`, `release-local.sh`, `get-testing.ps1`, `subset-fonts.sh` |
| `docs/bridge/` | The live HUD's bridge page (`index.html`, `sw.js`), served by GitHub Pages from `main` `/docs` at https://simulieren.github.io/deadtune/bridge/. The game's web panel loads it; it long-polls DeadTune's `hud::web_bridge` server on 127.0.0.1. Bump `PROTOCOL` (and the page's `VERSION`) and `sw.js`'s cache name together when the protocol changes. A push to `main` publishes it; keep it dependency-free with no requests anywhere but 127.0.0.1 |

## Commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo check -p dt-gui -p dt-cli --target x86_64-pc-windows-gnu --features dt-gui/fetch,dt-cli/fetch
```

All four must pass before you commit. Run the full workspace tests, not just your crate. Read the passed/failed totals; cargo stops at the first failing test binary. GitHub Actions is disabled, so the Windows cross-check above is the only Windows build gate on the Mac.

Open the app on a fake game with one command (builds with the quick `fast` profile, about 2 s after a change; the first build of a worktree takes about a minute):

```sh
scripts/app.sh                                   # fake install + throwaway data in target/
scripts/app.sh --images <Save all images folder> # previews and UI images page from an export
scripts/app.sh --fresh                           # new fake install and empty data
```

Or by hand:

```sh
G=$(scripts/fake-install.sh /tmp/dt-fake)
DEADTUNE_DATA_DIR=/tmp/dt-data cargo run -p dt-gui -- --game-dir "$G"
```

Build profiles: `dev` optimises dependencies (`opt-level = 2`) and dt-core a little (`opt-level = 1`), so debug runs and texture tests are fast while our own code still rebuilds in seconds. `fast` is release without LTO, for running locally. `release` (size-optimised, fat LTO) is only for shipping and takes minutes.

Use a throwaway `DEADTUNE_DATA_DIR`; never point experiments at the real data dir. Put `onboarded = true` in `<data>/settings.toml` to skip the welcome screen.

## Verify UI by looking

UI work is not done until you have produced screenshots and read them. Levers (read `crates/dt-gui/src/app.rs` and `main.rs` for the current set):

- `DEADTUNE_SCREENSHOT=<png>` renders a few frames, writes a PNG and exits. `--size WxH` sets the window.
- `DEADTUNE_SCREENSHOT_APPLY=1` presses Apply first.
- `DEADTUNE_SECTION=<name>` opens a simple-view section, `DEADTUNE_TAB=<name>` an advanced tab, `DEADTUNE_HUD_PAGE=colors|top|ingame|images` (Minimap colours, Top bar, In-game settings or UI images page), `DEADTUNE_HUD_SELECT`, `DEADTUNE_SEARCH`, `DEADTUNE_SCROLL=<px>`, `DEADTUNE_FAKE_WINDOWS=1` (Windows checks on sample facts), `DEADTUNE_SET=name=value,...`, `DEADTUNE_FAKE_GAME=1`, `DEADTUNE_FAKE_RUNNING=1` (game running, no polling; with `DEADTUNE_SCREENSHOT_APPLY=1` and a HUD edit it shows pak changes waiting for the game to close), `DEADTUNE_FAKE_CLOSE=1` (closes that faked game after the Apply, so the waiting paks go in), `DEADTUNE_FAKE_PUSH`, `DEADTUNE_FAKE_TRIAL=failed:<paks>|restored:hud|testing:<paks>|verified` (launch guard banner and HUD notes; paks are addon keys or `hud`), `DEADTUNE_FAKE_UPDATE`, `DEADTUNE_PRACTICE=shadows,fog,batching`, `DEADTUNE_BASE=<preset key>`, `DEADTUNE_OPEN_PRESETS=1` (holds the Overview's "All presets" dropdown open), `DEADTUNE_HEALTH_PRESET=<style>` (Health bar page on a style, its name lower case without spaces, e.g. `numberonly`), `DEADTUNE_HEALTH_LEVEL=<5..100>` (the health level its compare view starts at), `DEADTUNE_STATS_LOOK=<part>:<look>,...` (Player stats part looks, e.g. `items:rounded,souls:mono`), `DEADTUNE_FAKE_SNAPSHOT=running` (Game files page mid-snapshot). UI images page: `DEADTUNE_SECTION=images` (or `DEADTUNE_HUD_PAGE=images`), `DEADTUNE_IMAGES_FROM=<folder>` (preview a `snapshot take --images all` folder's `raw/panorama/images/` or a "Save all images" export, by its `manifest.json`, instead of pak01), `DEADTUNE_IMAGES_FOLDER=hud/top_bar`, `DEADTUNE_IMAGES_SEARCH=<text>`, `DEADTUNE_IMAGES_SELECT=<part of a path>`, `DEADTUNE_IMAGES_SET=<part of a path>=<png or svg>,...` (replace images), `DEADTUNE_IMAGES_ZOOM=1` (preview at 1:1), `DEADTUNE_IMAGES_EXPORT=running` ("Save all images" mid-way) or `=done`/`=zip` (runs the real export, with a zip for `zip`, before the capture), `DEADTUNE_SNAPSHOT_IMAGES=none|minimap_topbar|hud|all` (the Game files page's image scope). Colour edits: `DEADTUNE_IMAGES_EDIT=<part of a path>=<spec>[;<spec>],...` adjusts images, `DEADTUNE_IMAGES_BULK=<spec>[;<spec>]` adjusts every image the folder and search levers leave visible, `DEADTUNE_IMAGES_MARK=<part>,<part>` multi-selects tiles, `DEADTUNE_IMAGES_UNDO=<n>` undoes the last n page edits (shows the undo/redo state). Files in: `DEADTUNE_IMAGES_FIT=fill|stretch|own` sets the selected replacement's fit (with `DEADTUNE_IMAGES_SET` and `DEADTUNE_IMAGES_SELECT`), `DEADTUNE_IMAGES_IMPORT=<file, folder or zip>,...` receives them as if dropped (a collection waits in its review card), `DEADTUNE_IMAGES_HOVER=<path>,...` draws the drop overlay as if those files were dragged over the window. A spec is `Adjust::parse`'s spelling: `tint:#ff0000:80`, `colorize:#d4860b:100`, `overlay:#000000:multiply:50`, `hue:30`, `saturation:-40`, `brightness:20`, `contrast:10`, `opacity:50`, `invert`, `swap:#ece8e1:#4d75c3`. The capture waits for the page's thumbnails (and the HUD previews' pictures) to finish decoding. The fake install carries generated stand-in images in a few folders, one undecodable file, and a stand-in at every path the HUD previews draw (`dt_core::hud::art`). HUD previews: `DEADTUNE_PREVIEW_IMAGES=<folder>` draws them from a "Save all images" folder ahead of pak01 (also `preview_images` in `settings.toml`); `DEADTUNE_PREVIEW_SHAPES=1` draws only their fallback shapes. HUD layout page: `DEADTUNE_HUD_EDIT=minimap:-560:-180:140,top:0:60:80` moves (x, y in 1080p px) and sizes (percent) elements by label prefix; `DEADTUNE_HUD_BACKDROP=off` hides the game screenshot behind the boxes, `=60` sets its opacity. Live HUD preview: `DEADTUNE_FAKE_LIVE_HUD=off|not_installed|closed|waiting|waiting_long|waiting_page|stale|stale_base|live|error` puts the HUD pages' live indicator in that state (`docs/plans/live-hud/plan.md`). Check live preview: `DEADTUNE_FAKE_LIVE_CHECK=showing|shown|asking` (the visible test before and after the game shows the bigger minimap, the question) or a `dt_core::hud::live_check::sample` name (`works|old_pak|pending|no_condebug|script_error|no_page|fetch_blocked|port_busy|not_followed|not_seen`) for that checklist; `scripts/live-check-shots.sh [state ...]` renders every state on the HUD page (and `system:<state>` on System check) at 1280x800 and 1600x1000 into `target/shots/live-check/` as JPEGs ready to read. Live editing switch: `DEADTUNE_LIVE_EDITING=on|off` sets it for one run without saving; `scripts/live-editing-shots.sh` renders the HUD page live with the switch on and off at both sizes into `target/shots/live-editing/`. The screenshot (`crates/dt-gui/assets/vanilla_hud.jpg`) is the ground truth for the `Measured::Screenshot` rows of `hud::elements::ELEMENTS`; at Vanilla every outline must sit on its piece.

Check 1280x800 and 1600x1000. The screenshot PNGs are uncompressed; convert with `sips` before reading or committing them.

## Rules

- **TDD.** Failing test first, then the code. Use the real files under `research/` as fixtures.
- **Model the domain.** Enums and state machines over scattered booleans. Logic in `dt-core` or `state.rs` with tests, not in drawing code.
- **No new crates** without a stated reason and a measured binary-size cost. The Windows binary is about 8 MB; the target in `PLAN.md` is 5 MB.
- **Comments only for a non-obvious why.** No narrating comments, including in tests and scripts.
- **Plain language in the simple view.** No "convar", "gameinfo.gi" or "devonly" there. Errors are one human sentence plus what to do.
- **Credit authors.** Presets and addons always show their original author. Addons DeadTune rebuilds say "Method by <author>; rebuilt by DeadTune from your game files".
- **Never write a file DeadTune doesn't own.** Addon and HUD paks are recorded by sha256; foreign files are refused. Game files under `pak01` are never modified.
- **Glyphs.** The GUI ships only subsets of Inter and Hack (eframe `default_fonts` is off). A test fails if the UI draws a character the subset lacks. Add it to `scripts/subset-fonts.sh` and rerun it.

## Working alongside other agents

Several sessions work on this repo at once (core, HUD, updater). Avoid stepping on each other:

- **Work lands on `main` directly** (Simon, 2026-10-08): run the four gates, `git pull --rebase origin main`, push to `main`. No feature branches left for someone else to merge. A lone agent may work in the main checkout; parallel agents use a worktree each for isolation and push to `main` themselves. Do not leave uncommitted work in the main checkout at `~/Projects/Code/deadtune/deadtune-core`; another session's commit can sweep it up.
- Commit only the files you changed (`git add <paths>`, not `git add -A`), and check `git show --stat HEAD` after committing.
- Pull before merging. Resolve conflicts keeping both sides' intent, then rerun all gates.
- The git stash stack is shared across worktrees and sessions. Don't use bare `git stash`/`git stash pop`; prefer a WIP commit.
- Folders: the repo lives at `~/Projects/Code/deadtune/deadtune-core`. Other DeadTune work lives in sibling folders under `~/Projects/Code/deadtune/`. Don't move or rename folders while agents are building in them; run `git worktree repair .claude/worktrees/*` after a move.

## Releasing

Semver, minor-only for now: every release is `0.Y.0`, enforced by `dt-core`'s `version_is_minor_only` test and the release workflow. Releases are built on the Mac with MinGW by `scripts/release-local.sh`:

- `scripts/release-local.sh` replaces the rolling `testing` prerelease with committed, pushed HEAD.
- `scripts/release-local.sh minor` bumps the version, commits, tags, pushes and publishes a normal release.

Every release ships the zip, its sha256, the raw `deadtune-windows-x64.exe`, its minisign signature and `latest.json` for the in-app updater. The signing key lives only at `~/.config/deadtune/release.key`; the public key is `crates/dt-core/src/update/release.pub`. Never commit the private key. Release builds enable the `fetch` feature. Simon gets builds on Windows with `scripts\get-testing.ps1 -Tag latest -Run`. Details: `docs/releasing.md`.

## Hard-won facts

These cost real debugging. Don't relearn them.

- **SearchPaths.** Mounting addons needs `Game citadel/addons` before `Game citadel`, and `Mod`/`Write` pinned to `citadel` and `core` (Deadlock Mod Manager's layout). Stock gameinfo.gi has no `Mod` entry, so without the pins the engine takes the first `Game` path as MOD and dies with "Unable to read default keybinding configuration user_keys_default". `hud::searchpaths::ensure_addons` does this; check with `addons_ready`, not `has_addons`.
- **gameinfo.gi editing is lossless and line-based.** Only the `ConVars` block (and SearchPaths, as above) may change. Empty overrides must round-trip byte for byte, CRLF included. Valve's current file is tab-aligned with unquoted values (fixture `crates/dt-core/tests/fixtures/gameinfo_live_2026-09-29.gi`); Sqooky's "clean" copy is older and differently formatted.
- **Matchmaking.** Edited ConVars no longer block matchmaking. Since 2026-03 only edits to Engine2, MaterialSystem2, NetworkSystem, Particles, RenderSystem, SceneSystem and WorldRenderer do. Since 2026-09 the engine ignores 77 ConVars set in gameinfo.gi (`gameinfo_cannot_override`); the catalog marks them `gameinfo_ignored`.
- **Compiled stylesheets (`.vcss_c`).** The DATA block is a u32 CRC, a u16 image count, then per image a NUL-terminated name, u16 width, u16 height and (resource version 3 and up) a u32 CRC, then the CSS text. Keep the image table when rewriting. Reference: ValveResourceFormat `Panorama.Read`.
- **VPKs.** Our writer leaves the MD5 section zeroed; QoL Lite ships the same way and the game mounts it. Source2Viewer's verify complaint about it is expected.
- **Textures.** Downscaling by dropping the largest mips is a header patch plus a cut; no re-encode. Cubemaps, volumes, NO_LOD and non-mipmapped textures are declined.
- **VTEX extra data.** An extra-data entry's offset counts from its own offset field (entry + 4), like the header's extra-data offset; real files tile the DATA block exactly that way. Use `texture::vtex::extras`, never re-derive it. Until 2026-10-05 the parser counted from the entry start and misread every size table and display rect.
- **Game updates break rebuilt layouts.** The HUD pak carries whole copies of the game files it changes. Build 25738777 dropped `HudMinimapEffects` from `hud_minimap`, and our copy from 25712201 still named it, so the game died with "Unable to load layout file hud_minimap.xml". DeadTune now records the HUD layout with the pak and rebuilds it from the new game files (`hud::install::refresh_after_update`) at start, after an update and when the game closes. The launch guard keys its known-good set to the game build, so every pak is on trial again after an update. The record also keeps the sha256 of each game file the pak copies, so a changed file triggers a rebuild on the same build; a failed rebuild drops only the parts that rebuild layouts (`HudLayout::without_layout_rebuilds`). Addons built from game files record their settings and rebuild the same way (`addons::install::refresh_after_update`). Nothing rebuilds while the appmanifest's `StateFlags` say Steam is updating (`locate::steam_busy`). "Start with Windows" (`autostart`, `deadtune --background`) does this at sign-in without a window, so a game launched from Steam is covered too. On each new build's snapshot run `cargo run -p dt-core --example update_check -- <snapshot folder>`: it builds every HUD part and native addon against it and reads them back.
- **Particle stub.** Laund's particle disabler stub is the game's own `particles/empty.vpcf_c` (CRC `0xf3db7131`, 1189 bytes).
- **Line endings.** Fixtures are byte-exact golden files. The root `.gitattributes` disables conversion; a nested upstream `.gitattributes` once re-enabled CRLF on Windows checkouts. Don't add new ones under `research/`.
- **Console feedback.** Live pushes end with `echo DEADTUNE_ACK <nonce>` and convar queries; DeadTune reads them back from `console.log`, which the game writes when launched with `-condebug`.
- **Running game locks addon paks.** While Deadlock runs, writing `game/citadel/addons/pakNN_dir.vpk` fails with "Access is denied (os error 5)", even as administrator (Simon, 2026-10-06). Pak changes can only land while the game is closed, and the game has no Panorama reload command (`research/hud/panorama-runtime.md`), so live HUD changes must come from a script restyling panels, not from rewriting the pak. Apply therefore writes everything else and leaves pak changes waiting (`apply::PakTiming`, `PendingPaks` in `<records>/pending-paks.toml`); `crates/dt-gui/src/state/pending_paks.rs` puts them in when the game closes or DeadTune next starts. A pak write that still hits the lock is `HudError::Locked`/`AddonError::Locked` and waits too.
- **Panorama JS cannot read a ConVar, and the web panel loads only HTTPS.** `GameInterfaceAPI` does not exist in Deadlock, every `exec` from a script prints a console line, and hidden `CitadelSettingsSlider` panels load but read 0 (Simon, v0.17 to v0.26). Since the game's 2026-10-01 update `CitadelHTMLPanel.SetURL` ends at `about:blank` for `http://` and `data:` URLs. So the live HUD script opens DeadTune's bridge page on GitHub Pages (`docs/bridge/`), which long-polls `hud::web_bridge` on 127.0.0.1 (`/wait`, held up to 20 s) and hands messages over through `document.title` (`HTMLTitle` events, which can arrive twice); the script answers through the URL fragment and `$.Msg`. v0.27's 150 ms polling, a preflight per request (674 for 645 polls; the PNA preflight cache did not help) and a 4 Hz restyle cost real FPS (Simon, 2026-10-08), so the bridge now sleeps unless someone edits on a HUD page, and the script restyles only on messages (plus a slow beat for class-keyed rules). One web panel only: every extra Chromium panel costs frames. Never delete a web panel; deleting one seemed to stop the script. `crates/dt-core/tests/live_bridge.rs` runs the real script and page in Node (`tests/fixtures/live_hud_sim.js`) against DeadTune's real server. Design: `docs/plans/live-hud/plan.md`.
- **Never start Steam on the Mac.** `launch::open_url` refuses on macOS, so Launch, relaunch levers (`DEADTUNE_FAKE_GAME`) and tests can't start Simon's Steam client. Don't work around it.
- **Records folder.** The backup store and every addon, HUD, guard and practice record live in `backup::records_dir(data_dir)` (`<data>/backups`); presets, profiles, snapshots and bench history live in the data dir itself. The CLI once read records from the data dir and saw nothing the GUI wrote.
- **eframe features** are minimal on purpose: `glow`, `links` (hyperlinks don't open without it), `x11`, `wayland`.
- **Licensing.** The repo is public, GPL-3.0. Don't add code under non-free licences (DL-FOV-Fixer, PolyForm Noncommercial, was removed for this reason).
