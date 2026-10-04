# DeadTune: Deadlock Config Editor, Technical Plan

Working title: **DeadTune**. A tiny cross-platform app to edit Deadlock's performance config (`gameinfo.gi` ConVars + `video.txt`), apply changes live where the engine allows it, and compare presets quickly on a laptop.

Status: plan v1, 2026-10-04. Research bundle: `deadlock-research-core.zip` + `deadlock-gamebanana-files.zip` + `deadlock-vpk-addons.zip` + `deadlock-preset-screenshots.zip` (see "What we downloaded").

---

## 1. Goals and non-goals

**Goals**

- Edit any convar from the community presets with proper controls (toggle, slider, number, text), grouped and searchable.
- Start from a preset (Sqooky, Kaiz Min Spec, OptiLock Potato, vanilla) and layer personal overrides on top.
- Apply changes **live** when the engine allows it, otherwise queue them for a fast restart.
- Survive game updates: detect when Steam overwrites `gameinfo.gi` and re-apply in one click.
- Make A/B comparisons fast: same spot, two configs, FPS numbers + screenshots side by side.
- Smallest practical single-file binary, no installer, no runtime deps.

**Non-goals (hard lines)**

- No DLL injection, no memory reading/writing, no render hooking, no overlay inside the game process. Everything goes through files, launch options and the official console. This keeps us clear of anti-cheat territory.
- No touching gameplay-affecting or "wallhack-ish" convars (outlines through walls etc.). These are exactly what made Valve start restricting ConVars (see 2.4). The app ships a denylist.
- Not a mod manager. We coexist with Deadlock Mod Manager instead of replacing it.

---

## 2. What the research tells us

### 2.1 How the configs actually work

| File | Location | When read | What it holds |
|---|---|---|---|
| `gameinfo.gi` | `steamapps/common/Deadlock/game/citadel/` | Game launch | `ConVars { ... }` block with engine defaults. Also `SearchPaths` (used by mods). |
| `video.txt` | `.../game/citadel/cfg/` | Game launch, rewritten by the in-game video menu | `"setting.xxx" "value"` pairs: resolution, shadows, AA, upscaler, mip bias etc. |
| `*.cfg` | `.../game/citadel/cfg/` | When `exec`'d from console | Plain console commands. Needs verification for Deadlock (Phase 0). |

Both presets (Sqooky's OptimizationLock and dacooderr's OptiLock) are "replace the whole `gameinfo.gi`" configs. OptiLock also requires replacing the body of `video.txt`.

### 2.2 The key constraint: most tweaks need a restart

I cross-referenced every convar touched by the 8 downloaded presets against the community flag dump (`cvars_we_can_modify.txt`, 2,617 convars). Result for the 761 convars in the catalog:

| Apply class | Count | Meaning |
|---|---|---|
| `devonly` → **restart** | 445 | Can't be set from the console in release builds. Only works via `gameinfo.gi` at launch. This is *why* the presets edit gameinfo. |
| `cheat` → **live in hideout/sandbox** | 109 | Console-settable when cheats are on (Deadlock has `sv_cheats` on in local/hideout contexts; not in matchmaking). |
| plain → **live** | 65 | Console-settable anywhere. |
| unknown | 142 | Not in the dump (newer, renamed or removed). Treat as restart until verified. |

Takeaway: "live preview" is real but partial. The design must make **restart-required changes cheap**, not pretend everything is live. See section 5.

### 2.3 Existing tools worth learning from

| Project | What we take from it |
|---|---|
| **OptimizationLock** `auto updater/gameinfo_updater.py` (GPL-3) | The override model: lock a value / force-comment a convar, inject unknown ones in a managed block, timestamped backups, dry-run diff. Port this logic 1:1 to Rust. Their `overrides.gi` format becomes our import/export format. |
| **Deadlock Mod Manager** `packages/kv-parser` (Rust, GPL-3) | A KeyValues parser that preserves comments/whitespace (AST round-trip), plus diff/patch. Candidate for validation and for SearchPaths-safe edits. |
| **Deadlock Mod Manager** `game_config_manager.rs`, `steam_manager.rs` | SHA-256 tracked backups, CRLF preservation, mod-manager markers, game path via `steamlocate`, Flatpak Steam on Linux, **build id from `appmanifest`** (= cheap "game updated" detection). |
| **DL-FOV-Fixer** (Python tray app, PolyForm NC) | UX idea only: tray app that re-applies one value after updates. Don't copy code (license). |
| **Deadlock-Config** (Skip-eo) | Example `autoexec.cfg` layout and a commands spreadsheet. |

### 2.4 Risk: Valve's ConVar restriction

- The game ships the string: *"Unable to enter matchmaking while any party member has changes to ConVars in Gameinfo.gi or is running Tools-Mode."*
- Forum reports from March 2026 show players hitting it. Sqooky's README (updated Oct 3, 2026) says it is "not fully implemented" and the presets are still actively maintained.
- **Implication:** enforcement may be partial, rolling or toggled server side. The app must make it trivial to go back to vanilla (one click: "Ranked-safe mode", which restores the stock ConVars block and keeps `video.txt` tweaks, which are normal menu settings).
- Also note that `-tools` mode is explicitly named, which matters for the netcon workaround below.

### 2.5 Live console bridge options

| Option | Status | Notes |
|---|---|---|
| `-netconport <port>` TCP console | `-netconport` exists in the Deadlock binary's launch option list. In CS2 it works on Linux but is **broken on Windows** unless the game runs with `-tools` (WSAStartup ordering bug, open ~3 years). `-tools` blocks matchmaking. | Great on Linux/Steam Deck. On Windows likely only usable for offline tuning sessions. Verify in Phase 0. |
| `exec deadtune_live.cfg` via a bound key | Standard Source 2 behavior, needs verification for Deadlock. | App writes the file, you press one key in game. Works everywhere, zero risk. **Default fallback.** |
| Clipboard | Always works | App copies `cmd1; cmd2; ...`, you paste into console (F7). |

### 2.6 Platform reality

- Deadlock runs on **Windows** and **Linux / Steam Deck (via Steam/Proton)**. There is no native macOS build.
- So "cross-platform" means Windows x64 + Linux x64 as first-class targets. macOS (arm64) is a dev/build target only, unless you run Deadlock on the Mac through a compatibility layer, in which case paths need a custom override.

---

## 3. Stack decision

**Rust + egui (eframe), single binary.**

| Option | Size | Verdict |
|---|---|---|
| **Rust + eframe/egui 0.36** | ~3 to 6 MB | **Chosen.** No runtime deps, immediate mode is ideal for slider-heavy tweak UIs, works on Steam Deck without extra libs, cross-compiles cleanly. |
| Tauri 2 | ~3 to 10 MB | Smaller in theory but depends on WebView2 (Windows) / WebKitGTK (Linux). WebKitGTK on SteamOS is a pain. Deadlock Mod Manager uses Tauri and needs a full installer. |
| C/Zig + Dear ImGui | ~1 to 2 MB | Smallest but slowest to build features in. Not worth it for a couple of MB. |
| Go + Fyne/Wails | 15 to 30 MB | Too big. |

### 3.1 Crates

| Crate | Version (Oct 2026) | Use |
|---|---|---|
| `eframe` / `egui` | 0.36.2 | UI. Use the `glow` backend (smaller than `wgpu`). |
| `steamlocate` | 2.1.1 | Find Steam + Deadlock (appid `1422450`) across libraries. |
| `notify` | 8.2.0 | Watch `gameinfo.gi` / `video.txt` / `appmanifest_1422450.acf` for overwrites. |
| `similar` | 3.2.0 | Unified diff view before writing. |
| `serde` + `toml` | latest | Profiles. |
| `sha2` | latest | File hashes for backup/overwrite detection. |
| `sysinfo` (minimal features) | 0.39.x | Is `deadlock.exe` running? Process start time for restart loop. |
| `rfd` | 0.17.2 | Manual path picker fallback. Optional, adds size; can be replaced by a text field. |
| `tiny_http` | 0.12 | Optional `remote` feature: phone UI on LAN. |
| `keyvalues-parser` | 0.2.4 | Optional strict validation only (it does not preserve comments, so never use it to write). |

Deliberately **not** used for writing: any non-lossless KV serializer. Preset files are full of meaningful comments (978 lines carry `[def: "x"]` annotations), so we edit them line-by-line like the Python updater does.

### 3.2 Size budget

```toml
[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

- eframe with `default-features = false, features = ["glow", "default_fonts"]`. Consider dropping `default_fonts` and embedding one small subset font.
- Target: **≤ 5 MB** Windows, **≤ 6 MB** Linux. Track size in CI and fail if it grows > 10% per PR.
- No UPX (antivirus false positives on a tool that writes into game folders is the worst possible look).

---

## 4. Architecture

```
deadtune/
├─ crates/
│  ├─ dt-core/          # no UI, fully unit-tested
│  │  ├─ locate.rs      # steam + game path, appmanifest build id
│  │  ├─ gi.rs          # lossless ConVars block editor (port of gameinfo_updater.py)
│  │  ├─ video.rs       # video.txt reader/writer (keep deviceid header, replace settings)
│  │  ├─ catalog.rs     # convar metadata: type, range, default, apply class, category, notes
│  │  ├─ profile.rs     # base preset + overrides (TOML), import/export overrides.gi
│  │  ├─ backup.rs      # timestamped backups, sha256, restore, "vanilla" snapshot
│  │  ├─ watch.rs       # notify-based overwrite + update detection
│  │  ├─ apply.rs       # plan: what is live vs restart, builds console batch + file writes
│  │  ├─ bridge/
│  │  │  ├─ netcon.rs   # TCP console client
│  │  │  ├─ execfile.rs # writes cfg/deadtune_live.cfg
│  │  │  └─ clipboard.rs
│  │  ├─ launch.rs      # steam://rungameid/1422450 + launch options, kill/relaunch
│  │  └─ bench.rs       # import PresentMon (Win) / MangoHud (Linux) CSV, compute avg/1% low
│  ├─ dt-gui/           # eframe app
│  └─ dt-cli/           # same core as CLI: apply, restore, diff, watch (tiny, scriptable)
├─ catalog/             # convar_catalog.csv -> curated catalog.toml (embedded at build)
└─ presets/             # fetched at runtime by default (see licensing)
```

### 4.1 `gi.rs`: the lossless editor

Port the proven algorithm from `gameinfo_updater.py`:

1. Find `ConVars {` and its matching `}` with comment-aware brace counting.
2. Walk body lines; skip nested blocks.
3. Match each line with the convar regex: `indent, optional //, name, pad, value, optional trailing comment`.
4. For each override: `Set(value)` rewrites value (and uncomments), `Comment` adds `// `.
5. Unknown overrides go into a managed block between marker comments, stripped and regenerated on every apply.
6. Preserve line endings (CRLF on Windows), indentation and trailing comments.
7. Validate braces before and after. Refuse to write if validation fails.
8. Never touch anything outside `ConVars` (so Deadlock Mod Manager's `SearchPaths` edits survive).

Parse the trailing comments into metadata: `[def: "x"]` → default value, rest → description. Seed the catalog from this automatically.

### 4.2 Catalog: making 700+ convars usable

Each entry in `catalog.toml`:

```toml
[r_farz]
category   = "Render distance"
type       = "float"          # bool | int | float | enum | string
range      = [2000, 16384]
step       = 500
default    = -1
apply      = "restart"        # live | live_cheat | restart
impact     = "high"           # curated: high | medium | low | unknown
denylist   = false
notes      = "Far clip plane. Lower = buildings/players pop in."
```

Bootstrapping:

1. Auto-generate from `data/convar_catalog.csv` (flags, defaults, preset comments, values per preset).
2. Infer type from values seen (`true/false` → bool, integers, floats).
3. Hand-curate the ~60 convars that matter most (shadows, lighting, particles, LOD, far-z, panorama, threads). The rest live under "Advanced" with a raw editor.
4. Denylist: anything that changes visibility through walls, outlines, or other competitive info.

### 4.3 Profiles

```toml
name = "Laptop / battery"
base = "kaiz_minspec"            # or sqooky, optilock_potato, vanilla, file:path
base_rev = "a1b2c3"              # git sha / hash of the base when profile was saved

[convars.set]
r_farz = "6000"
fps_max = "60"

[convars.comment]
names = ["citadel_camera_hero_fov"]

[video]
"setting.r_texture_stream_mip_bias" = "4"
"setting.fps_max" = "60"
```

- Import/export Sqooky's `overrides.gi` format so users of his updater can switch both ways.
- Two built-in suggestions: "Plugged in" and "Battery" (fps cap, lower particles, panorama fps cap).
- Optional auto-switch by power state (Windows `GetSystemPowerStatus`, Linux `/sys/class/power_supply`). Since gameinfo is read at launch, the switch happens when the game is not running, before launch.

### 4.4 Apply engine

When you hit Apply:

1. Diff current effective state vs desired.
2. Split changes by `apply` class:
   - `live` → console batch now (via the active bridge).
   - `live_cheat` → console batch only if "in hideout/sandbox" is ticked, else queue.
   - `restart` → write to `gameinfo.gi` / `video.txt` now; mark "pending restart" in the UI.
3. Always also persist live changes to `gameinfo.gi` so they survive restarts.
4. Show the diff, write atomically (temp file + rename), store a backup, record hashes.

---

## 5. Preview strategy (the "quick previews" part)

Three tiers, cheapest first:

**Tier 1: Live tweak (seconds).** For `live` / `live_cheat` convars. Sliders send values as you drag (debounced ~150 ms) over netcon, or on "Push" via the exec-file key bind. Best place: Hideout or a sandbox match.

**Tier 2: Fast restart loop (~30 to 60 s).** For `restart` convars.
- "Apply + relaunch" button: write files → close game → `steam://rungameid/1422450` with saved launch options (`-novid`, plus a straight-to-sandbox map if Phase 0 finds a working `+map` target).
- The app shows a countdown and the list of changes being tested.
- Optional "batch" mode: queue several restart changes, test them together.

**Tier 3: A/B benchmark (minutes, objective).**
- Record a fixed route (e.g. 60 s walk through the same lane in sandbox) under config A, then B.
- Capture frametimes with **PresentMon** (Windows, Intel, open source CLI) or **MangoHud** logging (Linux/Deck). The app launches the capture tool and imports the CSV.
- Show avg FPS, 1% low, 0.1% low, frametime graph overlaid, plus Steam screenshots (F12) from each run side by side. Screenshots path is `userdata/<id>/760/remote/1422450/screenshots`.
- Store results per profile so you build a history ("Kaiz + farz 6000 = +11% 1% lows on battery").

### 5.1 "While in game" UX without an overlay

- Compact always-on-top mode (egui viewport `with_always_on_top`), ~320 px wide, only search + favourite sliders + Push. Use with borderless windowed.
- **Remote mode** (`--features remote`): `tiny_http` serves a single embedded HTML page on the LAN (QR code shown in the app) so you tweak from your phone while the game stays fullscreen. Bind to LAN IP only, random token in the URL.
- Global hotkey (optional `global-hotkey` crate) to cycle profiles or trigger Push.

---

## 6. Safety and robustness

- First run: snapshot the current `gameinfo.gi` and `video.txt` as "original" (sha256 recorded). Never overwritten.
- Every write: timestamped backup, keep last 20.
- Detect game updates by watching `appmanifest_1422450.acf` `buildid` and the gameinfo hash. On change: banner "Game updated, your config was overwritten. Re-apply?" with a diff of what Valve changed in the stock ConVars block (useful when they add new convars).
- If the game is running: allow writes (they take effect next launch) but label them clearly.
- "Ranked-safe mode": one click restores stock ConVars and keeps `video.txt`. One click back to your profile.
- Mod Manager coexistence: only edit inside `ConVars`. Detect DMM markers and warn if DMM later rewrites the file.
- Never write if brace validation fails or the ConVars block is missing.

---

## 7. Licensing

- OptimizationLock, OptiLock and Deadlock Mod Manager's kv-parser are all **GPL-3.0**. DL-FOV-Fixer is PolyForm Noncommercial (do not reuse code).
- Simplest path: **license DeadTune as GPL-3.0**, credit Sqooky / Kaizuchaneru / Boot / dacooderr per preset in the UI.
- Presets: fetch from upstream GitHub at runtime (with a pinned fallback copy) rather than baking them in. Keeps authors' updates flowing and sidesteps the Sqooky vs dacooderr attribution dispute; show both with clear authorship.

---

## 8. Phased plan

### Phase 0: Verification spikes (1 to 2 evenings, on the actual laptop)

Each one is a yes/no that changes the design:

1. Does `exec <file>` from `game/citadel/cfg` work in Deadlock? Does `autoexec.cfg` run?
2. Does `-netconport 2121` accept a TCP connection on Windows **without** `-tools`? On Linux/Deck?
3. With a modified ConVars block, can you still queue matchmaking today? (Defines how loud "Ranked-safe mode" must be.)
4. Which of 10 sample `cheat` convars work live in Hideout? Which `devonly` ones are truly rejected live?
5. Is there a launch option that drops you straight into sandbox/hideout (`+map <name>`)? Cuts restart loop time.
6. Does the game re-read `video.txt` without restart (e.g. after toggling a menu setting)? Probably not, but check.
7. Baseline: PresentMon/MangoHud capture works and gives stable numbers over 3 identical runs (variance < 3%).

Deliverable: a short findings note + updated `apply` classes for the curated convars.

### Phase 1: Core + CLI (~1 week of evenings)

- `locate`, `gi` (port + tests using all 8 downloaded presets as golden files: round-trip must be byte-identical when no overrides), `video`, `backup`, `profile`, `catalog` generator.
- `dt-cli apply --profile laptop.toml`, `restore --original`, `diff`, `watch`.
- CI matrix: `windows-latest`, `ubuntu-latest`, `macos-latest`. Release artifacts + size report.

### Phase 2: GUI MVP (~1 to 2 weeks)

- Layout: left = categories + search, center = convar rows (control, value, default, preset values, apply-class badge, impact), right = pending changes + diff.
- Preset picker, profile save/load, Apply, Ranked-safe toggle, update banner.
- Compact always-on-top mode.

### Phase 3: Live bridge (~1 week)

- exec-file bridge (default), netcon bridge (auto-detect), clipboard fallback.
- Debounced slider push, "in hideout" toggle for cheat convars.
- Apply + relaunch loop.

### Phase 4: Bench + screenshots (~1 week)

- PresentMon/MangoHud launch + CSV import, metrics, overlay chart (egui_plot, check size impact).
- Screenshot pairing per run, history per profile.

### Phase 5: Nice to have

- Remote phone UI.
- Power-state auto profile switch.
- Upstream preset change feed ("Sqooky changed 6 convars since your base_rev").
- Share profile as a short string/QR.

---

## 9. Open questions for you

1. **Which laptop runs Deadlock?** Windows gaming laptop, Linux/Deck, or the MacBook through a compatibility layer? This decides whether netcon is viable and which bench tool we wire first.
2. GPU vendor (NVIDIA / AMD / Intel iGPU)? Some convars and upscalers (DLSS vs FSR) only matter on one vendor.
3. Do you still want to play ranked with the tweaks? If yes, Phase 0 item 3 is the first thing to test.
4. Open source on GitHub from day one (GPL-3) or private until MVP?

---

## 10. What we downloaded

Split into four zips to fit upload limits: **core** (all text: presets, dumps, scripts, reference code, catalog), **gamebanana-files**, **vpk-addons** (duplicate VPKs that exist in both repos included once), **preset-screenshots** (recompressed JPGs).

| Path | Source | Notes |
|---|---|---|
| `configs/OptimizationLock/` | github.com/Sqooky/OptimizationLock (commit 2026-10-03) | All presets, `auto updater/` (Python), `cvars_we_can_modify.txt` (flags), `convars.txt`, `cvarlist.txt`, `launch_options.txt` (1,536 options), performance VPK addons, screenshots. |
| `configs/OptiLock/` | github.com/dacooderr/OptiLock (commit 2026-10-02) | Recommended + Potato presets with `video.txt`, `cvarlist.md`, Essential Fixes VPKs, localized presets. |
| `gamebanana/optilock_fps_configs.zip` | GameBanana 678180 | OptiLock configs as published there (Oct 1). |
| `gamebanana/qollite.zip` | GameBanana 678180 | QoL Lite VPK (Oct 3). Binary mod, HUD/QoL. |
| `gamebanana/qollock.zip` | GameBanana 650634 | QOL Lock 4.0.4 VPK (Oct 3). Binary mod, HUD/UI overhaul. Announcer add-ons skipped. |
| `reference/deadlock-mod-manager-excerpts/` | github.com/deadlock-mod-manager/deadlock-mod-manager | Rust `kv-parser` package + `game_config_manager.rs` + `steam_manager.rs` + license. |
| `reference/DL-FOV-Fixer/` | github.com/lukr-99/DL-FOV-Fixer | Reference only (non-commercial license). |
| `reference/Deadlock-Config/` | github.com/Skip-eo/Deadlock-Config | autoexec example + commands spreadsheet. |
| `data/convar_catalog.csv` | Generated | 761 convars × apply class, flags, engine default, description, preset comment, value in each of 8 presets. Seed for `catalog.toml`. |

The GameBanana "Gameinfo.gi repair tool [WIP]" (mod 651416) has no downloadable files anymore, so nothing to grab there.

---

## Sources

- [Sqooky/OptimizationLock](https://github.com/Sqooky/OptimizationLock)
- [dacooderr/OptiLock](https://github.com/dacooderr/OptiLock)
- [QoL Lite on GameBanana](https://gamebanana.com/mods/678180)
- [QOL Lock on GameBanana](https://gamebanana.com/mods/650634)
- [Deadlock Mod Manager](https://github.com/deadlock-mod-manager/deadlock-mod-manager)
- [DL-FOV-Fixer](https://github.com/lukr-99/DL-FOV-Fixer)
- [Skip-eo/Deadlock-Config](https://github.com/Skip-eo/Deadlock-Config)
- [Forum: "Valve PLEASE optimize the game before you disallow configs" (Mar 2026)](https://forums.playdeadlock.com/threads/valve-please-optimize-the-game-before-you-disallow-configs.122053/)
- [CS2 issue #3603: -netconport broken on Windows](https://github.com/ValveSoftware/csgo-osx-linux/issues/3603)
- [Steam guide: Deadlock console](https://steamcommunity.com/sharedfiles/filedetails/?id=3654441640)
- crates.io pages for eframe, steamlocate, notify, similar, keyvalues-parser, tiny_http
