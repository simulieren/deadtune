# Game file snapshots

Status: built (core, CLI, GUI page, auto snapshot); proven on the fake install only, the real game is checked in `docs/testing-windows.md` section 19. Last update 2026-10-05.

## Goal

DeadTune rebuilds every HUD and addon file from the player's own `pak01_dir.vpk`. When Valve ships a big update, those game files change under us and we find out through breakage. A snapshot copies the files DeadTune cares about (HUD layouts and stylesheets, the settings menu, the main menu, `gameinfo.gi`, the cfg files) out of the game into a dated folder, decodes the compiled ones to readable text, and records every file's size and CRC. Two snapshots diff into a report that says what changed and which DeadTune feature depends on it. The in-game settings plan (`docs/plans/ingame-settings/plan.md`, Phase 1) needs the same thing as fixtures, starting with the game's current `popup_settings.vxml_c`.

Everything here is read-only on the game. Snapshots live in DeadTune's data folder.

## What gets exported

A `SnapshotSpec` is a set of categories. Each category is a path rule over the pak01 file list (`hud::vpk::VpkDir`, tree only, so counting is instant) plus a few loose files. A file can be in several categories; the export is the union.

| Category | Rule | Default |
|---|---|---|
| HUD | `panorama/{layout,styles,scripts}/` files whose name starts with `hud`, `citadel_hud`, `minimap`, `top_bar` or `health` | on |
| Settings menu | the same folders, name contains `settings` (covers `popups/popup_settings*`) | on |
| Main menu | the same folders, name starts with `base` or contains `dashboard` or `main_menu` | on |
| Everything Panorama | every `.vxml_c`, `.vcss_c`, `.vjs_c` under those three folders (text-sized, the whole UI) | on |
| DeadTune targets | every pak path a DeadTune generator reads, derived from the code (`Native::game_files()` for each native addon in the `ADDONS` registry, the HUD style and layout constants, the planned settings layout) | on |
| Config | `gameinfo.gi`, `cfg/*.cfg`, `cfg/*.vcfg`, `cfg/*.txt`, the Steam `appmanifest_1422450.acf` (buildid) | on |

Models, sounds, maps and textures stay out unless a DeadTune generator reads them (the scope texture, the Sinner mask and model, the empty particle). A size cap (default 8 MB) keeps big binaries out: a file over the cap is still listed in the manifest with its CRC, so the diff sees it change, but stores nothing. Files a DeadTune feature reads (the `deadtune` category, including `gameinfo.gi` and `video.txt`) ignore the cap, so a snapshot always holds every generator input; the 16 MB scope texture is the one that needs it. DeadTune 0.9.0 stored only a header (`stored = "header"`, `raw/<path>.header`) for textures over the cap; a later take of the same build replaces it.

"ConVar info" means what the files hold: the `ConVars` block of `gameinfo.gi` and the archived values in the cfg files. The engine's full ConVar list only exists in a running game (`cvarlist`), outside a snapshot.

On top of the categories, every snapshot writes `pak01.tsv`, one line per pak01 entry with its size and CRC. That is a few megabytes of text and lets a diff count what changed in the rest of the game (models, particles, maps) without copying any of it.

## Snapshot layout

```
<data dir>/game-files/<buildid>-<YYYYMMDD-HHMMSS>/
  manifest.toml        buildid, date, DeadTune version, options, one entry per file
  pak01.tsv            every pak01 entry: path, size, crc
  raw/<path>           bytes as in the game (pak paths as-is, loose files relative to game/citadel, the appmanifest under steam/)
  text/<path>          decoded: .vcss_c -> .css, .vxml_c -> .xml, .vjs_c -> .js
  text/<path>.strings.txt   fallback when a file cannot be decoded: its readable strings and the error
  diff-<old>-to-<new>.md    written by a compare into the newer snapshot
  diff-<old>-to-<new>.toml  the same, machine-readable
```

Manifest entry:

```toml
[[files]]
path = "panorama/layout/citadel_hud_top_bar.vxml_c"
source = "pak01"            # pak01 | loose | steam
size = 3154
crc = "7a1c0f33"            # VPK tree CRC; crc32 of the bytes for loose files
sha256 = "..."
categories = ["hud", "panorama", "deadtune"]
stored = "full"             # full | none (over the size cap) | header (0.9.0 only)
text = "panorama/layout/citadel_hud_top_bar.xml"
decoded = "text"            # text | strings | none
```

Decoding uses what DeadTune already has: `hud::resource::style_text` for stylesheets, the DATA block for scripts, `hud::inject::layout_text` (KV3 `LaCo` to XML) for layouts. `hud::kv3` now also reads the legacy `VKV\x03` binary format (the one the Wide FOV mod's `popup_settings.vxml_c` uses, `Kv3(Magic(55987030))` before). The game's own layouts use version 5, which it already read.

Export streams one file at a time with a progress callback that can cancel. Writes are atomic. A snapshot folder is keyed by buildid: taking a snapshot of a build that already has one reuses its folder, skips every file whose CRC and bytes are already there, and adds what is new (more categories, a changed file). `manifest.toml` is written last, so a folder without one is incomplete and the next take fills it in. Cancelling a fresh snapshot removes its folder.

## Diff

`snapshot::diff::compare(old, new)` reads both manifests and lists, per category, the files added, removed and changed (by CRC). For a changed file with decoded text on both sides it carries a unified diff of the text (`similar`, the crate the apply preview already uses). The DeadTune impact section flags every added, removed or changed file a feature depends on, with the feature's name: HUD layout, Minimap (icons, apples and tunnels), Top bar, Health bar, each native addon (blur, particle stub and clutter, Sinner, scope), the planned settings menu rows, and `gameinfo.gi` and `video.txt` for the ConVar editor. The dependency table is built from the same constants the generators use, so a new generator that reads a new file shows up here by adding its constant, not a string.

When both snapshots have `pak01.tsv`, the report also counts changes outside the snapshot, grouped by top folder ("models 812 changed, particles 95 added").

The report is written into the newer snapshot's folder as Markdown and TOML. The Markdown is what "Copy summary" copies and what the CLI prints.

## UI

A "Game files" page in the simple view's sidebar under More, between System check and Safety & setup. Plain language, no "convar" or "VPK" on the page.

- Header card: the game's build id, when the last snapshot was taken and for which build.
- "What to save": one checkbox per category with its file count and size (from the pak tree, computed when the page opens and after a game update), "Decode to readable text", and "Skip files bigger than" (1, 4, 8, 32 MB or no limit). The choices persist in settings.
- Buttons: Take snapshot (progress bar with the current file and Cancel while it runs), Compare with previous (two pickers, newest two by default), Open snapshot folder, Copy summary.
- Results: the latest report. Counts per category, the DeadTune impact list in amber naming the feature at risk, expandable lists of added, removed and changed files, each with Open (raw) and Open text.
- "Snapshot automatically after game updates", on by default. When the watcher reports a buildid change, the app takes a snapshot in the background and diffs it against the previous one; the results card shows it.
- Snapshots: every folder with build, date, file count and size, Open and Delete.

Screenshot levers: `DEADTUNE_SECTION=game` opens the page; `DEADTUNE_FAKE_SNAPSHOT=running` shows the progress state.

## CLI

```
deadtune-cli snapshot take [--categories hud,settings,menu,panorama,deadtune,config] [--no-decode] [--size-cap <MB>]
deadtune-cli snapshot list
deadtune-cli snapshot diff <a> <b>      # folder names or build ids; "latest" and "previous" work too
```

`take` prints the folder, file count, bytes and time. `diff` writes the report into the newer snapshot and prints it.

## Design notes

Choices made before writing code, with the alternatives considered.

| Decision | Chosen | Alternatives |
|---|---|---|
| Category membership | one table of rules, a file can match several, the manifest lists all of them | one category per file (loses "this HUD file is also a DeadTune target"); user-typed globs (nothing to count instantly, nothing to test) |
| Folder key | buildid plus a timestamp, reuse the folder for the same build | timestamp only (two snapshots of one build, diffs of nothing); buildid only (loses when it was taken) |
| Big binaries | list in the manifest, store nothing or the texture header | store everything (16 MB scope texture per snapshot); drop them from the list (the diff goes blind on the scope) |
| Decoded text | written next to raw, diffs read the text files | decode at diff time (slower, and the point of the folder is to open the files) |
| Dependency list | built from the generators' path constants and the `ADDONS` registry | a hand list of strings (drifts the first time a generator changes) |
| Whole-game change count | `pak01.tsv` per snapshot | none (a settings-only snapshot cannot say "the update touched 4,000 models") |
| Background work in the GUI | one job thread with a channel and a cancel flag, the texture build's shape | blocking the UI (minutes on the real game); async runtime (a new crate) |

## Phases

1. Core: `snapshot::spec` (categories, rules, inventory), `snapshot::export` (take, manifest, decode, strings fallback, texture header), `snapshot::store` (folders, list, delete). Tests on a fake pak and a fake install. Done.
2. Core: `hud::kv3` legacy `VKV\x03` reader so the Wide FOV sample decodes; checked against the sample outside the repo (`DEADTUNE_KV3_SAMPLES`) and a hand-built fixture in the tests. Done.
3. Core: `snapshot::diff` with the DeadTune impact table, Markdown and TOML reports. Done.
4. CLI: `snapshot take | list | diff`. Done.
5. GUI: the page, the job, the setting, the auto snapshot on a game update, screenshots. Done.
6. Windows: take a snapshot of the real game, note size and time, open the decoded `popup_settings.xml`, and after the next update check the automatic snapshot and its report (`docs/testing-windows.md`, section 19).
7. Later, when the in-game settings work needs it: copy the real `popup_settings.vxml_c` from a snapshot into the fixtures.

## Risks

- The real pak01 tree is large (tens of thousands of entries). Opening it reads only the header and tree; counting is a pass over a map. Measured on the fake install only (16 files, 3.0 MB stored, 0.4 s); the Windows check records the real numbers.
- A game update that changes the compiled resource header version or the KV3 version breaks decoding. The strings fallback keeps the raw bytes and the readable strings, and the diff still works on CRCs; only the text diff is lost. The manifest says `decoded = "strings"` so it is visible.
- Steam writes the new buildid at the end of an update, so the automatic snapshot should see complete files. If a pak chunk is still being written, a CRC error on that file is reported and the file is skipped; the next take fills it in.
- Snapshots are a few tens of megabytes each with Everything Panorama on. The page lists them with sizes and a Delete button; nothing is pruned automatically.
- `explorer`, `open` and `xdg-open` on a file open it with the default app. On Windows an `.xml` may open in a browser. Good enough for reading.
