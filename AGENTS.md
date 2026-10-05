# AGENTS.md

Guidance for coding agents working on DeadTune. Read this before changing anything. Product spec: `PLAN.md`. User docs: `README.md`. Release process: `docs/releasing.md`. Windows test checklist: `docs/testing-windows.md`. HUD design: `docs/plan-hud.md`.

## What this is

DeadTune is a Windows desktop app (Rust, egui/eframe 0.36) that tunes Deadlock's performance settings: the `ConVars` block of `game/citadel/gameinfo.gi`, `cfg/video.txt`, HUD layout and colours, and performance addons. It works only through files, launch options and the official console. No DLL injection, memory access or hooks. Competitive-information ConVars (outlines through walls, visibility) are on a denylist and never changed. HUD features that re-present information through the game's own UI (top bar dimming, spawn timers, urn soul lead, purchase popups) are allowed: Simon decided on 2026-10-05 that top bar information features are safe. They stay opt-in and off in the Vanilla preset.

The game runs on Windows (primary) and Linux/Proton. Development happens on macOS, which cannot run the game. Anything that touches the real game is proven on Windows by Simon, using `docs/testing-windows.md`.

## Layout

| Path | Contents |
|---|---|
| `crates/dt-core` | All logic, no UI, fully unit-tested: `gi` (lossless ConVars editor), `video`, `catalog`, `preset`, `profile`, `apply` (pure plan, then effects), `backup`, `locate`, `watch`, `launch`, `power`, `bridge/` (exec-file, netcon, clipboard, console ack and log tail, boot cfg), `bench`, `doctor`, `hud/` (VPK reader/writer, compiled resources, Panorama CSS, layout, SearchPaths, minimap colours, UI image overrides), `addons/` (registry, install, verify, launch guard, generators), `texture/` (vtex parser, mip stripping, PNG/SVG encoding into game images), `update/` (signed self-update), `winfps/` (read-only Windows FPS checks), `snapshot/` (game file snapshots and diffs, `docs/plans/game-files/plan.md`) |
| `crates/dt-gui` | eframe app, binary `deadtune`. `state.rs` holds `AppState` and every transition, with tests. Views are thin: `simple.rs` (sidebar view), `advanced.rs`/`views.rs`, `compact.rs` (mini window), `hud_view.rs`, `minimap_view.rs`, `addons_view.rs`, `game_files_view.rs` (with `snapshots.rs` for its job), `update_view.rs`, `live_status.rs`, `theme.rs` |
| `crates/dt-cli` | Binary `deadtune-cli` over the same core: `doctor`, `diff`, `apply`, `ranked-safe`, `restore`, `push`, `launch`, `hud`, `addons`, `bench`, `snapshot`, `self-update`, etc. Hand-rolled arg parsing |
| `catalog/` | `catalog.toml` is generated from `research/data/convar_catalog.csv` plus `curated.toml`. Never edit it by hand: run `cargo run -p dt-core --example gen_catalog`. A test fails on drift |
| `research/` | Upstream presets (GPL-3.0), mod VPKs, convar dumps, reference code, HUD research. Golden fixtures for tests |
| `scripts/` | `fake-install.sh`, `release-local.sh`, `get-testing.ps1`, `subset-fonts.sh` |

## Commands

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo check -p dt-gui -p dt-cli --target x86_64-pc-windows-gnu --features dt-gui/fetch,dt-cli/fetch
```

All four must pass before you commit. Run the full workspace tests, not just your crate. Read the passed/failed totals; cargo stops at the first failing test binary. GitHub Actions is disabled, so the Windows cross-check above is the only Windows build gate on the Mac.

Run the GUI against a fake game:

```sh
G=$(scripts/fake-install.sh /tmp/dt-fake)
DEADTUNE_DATA_DIR=/tmp/dt-data cargo run -p dt-gui -- --game-dir "$G"
```

Use a throwaway `DEADTUNE_DATA_DIR`; never point experiments at the real data dir. Put `onboarded = true` in `<data>/settings.toml` to skip the welcome screen.

## Verify UI by looking

UI work is not done until you have produced screenshots and read them. Levers (read `crates/dt-gui/src/app.rs` and `main.rs` for the current set):

- `DEADTUNE_SCREENSHOT=<png>` renders a few frames, writes a PNG and exits. `--size WxH` sets the window.
- `DEADTUNE_SCREENSHOT_APPLY=1` presses Apply first.
- `DEADTUNE_SECTION=<name>` opens a simple-view section, `DEADTUNE_TAB=<name>` an advanced tab, `DEADTUNE_HUD_PAGE=colors` (Minimap page), `DEADTUNE_HUD_SELECT`, `DEADTUNE_SEARCH`, `DEADTUNE_SCROLL=<px>`, `DEADTUNE_FAKE_WINDOWS=1` (Windows checks on sample facts), `DEADTUNE_SET=name=value,...`, `DEADTUNE_FAKE_GAME=1`, `DEADTUNE_FAKE_PUSH`, `DEADTUNE_FAKE_TRIAL`, `DEADTUNE_FAKE_UPDATE`, `DEADTUNE_PRACTICE=shadows,fog,batching`, `DEADTUNE_BASE=<preset key>`, `DEADTUNE_OPEN_PRESETS=1` (holds the Overview's "All presets" dropdown open), `DEADTUNE_FAKE_SNAPSHOT=running` (Game files page mid-snapshot).

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

- **Use a git worktree or a separate clone for any change**, and merge into `main` when done. Worktrees are explicitly allowed in this repo. Do not leave uncommitted work in the main checkout at `~/Projects/Code/deadtune/deadtune-core`; another session's commit can sweep it up.
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
- **Particle stub.** Laund's particle disabler stub is the game's own `particles/empty.vpcf_c` (CRC `0xf3db7131`, 1189 bytes).
- **Line endings.** Fixtures are byte-exact golden files. The root `.gitattributes` disables conversion; a nested upstream `.gitattributes` once re-enabled CRLF on Windows checkouts. Don't add new ones under `research/`.
- **Console feedback.** Live pushes end with `echo DEADTUNE_ACK <nonce>` and convar queries; DeadTune reads them back from `console.log`, which the game writes when launched with `-condebug`.
- **eframe features** are minimal on purpose: `glow`, `links` (hyperlinks don't open without it), `x11`, `wayland`.
- **Licensing.** The repo is public, GPL-3.0. Don't add code under non-free licences (DL-FOV-Fixer, PolyForm Noncommercial, was removed for this reason).
