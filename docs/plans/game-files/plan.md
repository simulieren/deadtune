# Game file snapshots

Status: built (core, CLI, GUI page, auto snapshot); proven on the fake install only, the real game is checked in `docs/testing-windows.md` section 19. UI images (section "UI images" below) are built in core and the CLI; the GUI picker and the preview browser are next. Last update 2026-10-05.

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

Models, sounds, maps and textures stay out unless a DeadTune generator reads them (the scope texture, the Sinner mask and model, the empty particle) or the image scope below includes them. A size cap (default 8 MB) keeps big binaries out: a file over the cap is still listed in the manifest with its CRC, so the diff sees it change, but only a `.vtex_c` gets its header stored (`raw/<path>.header`, everything before the pixel data) and other files store nothing. Images in scope are exempt from the cap.

### UI images

`panorama/images/**` holds 2706 files in build 25712201 (2058 `.vtex_c` textures, 648 `.vsvg_c` vector icons, 3.3 GB; hud 604, heroes 525, upgrades 220, icons 204, items 181, ranked 170, shop 134, main_menu 103, minimap 86). They are an opt-in scope on top of the categories, `Selection::images`, with nesting levels:

| `ImageScope` | What it adds |
|---|---|
| `none` (default) | nothing; the scope texture a generator reads still comes in through DeadTune targets, header only over the cap |
| `minimap_topbar` | `panorama/images/minimap/**`, `panorama/images/hud/top_bar/**`, plus every image the minimap and top bar layouts, stylesheets and scripts name (`Inventory::referenced_images`, read from the pak when the inventory is built: `s2r://panorama/images/x.vtex` and `file://{images}/x.png` references, mapped to their compiled names) |
| `hud` | all of `panorama/images/hud/**` and the level above |
| `all` | every `panorama/images/**` file |

Every image gets the `images` category in the manifest (`Category::Images`, not one of the six checkbox categories in `Category::ALL`). Decoding writes `text/<path>.png` (top mip, through `texture::decode` and `texture::png`) and `text/<path>.svg` (the SVG source out of the `.vsvg_c` container, `texture::svg::decode_svg`) with the compiled extension replaced, the same way `.vcss_c` becomes `.css`, so a snapshot folder browses in Finder or Explorer. The manifest entry carries `decoded = "image"` and an `image` table (`width`, `height`, `format`, `mips`, from the texture header, the FILL_TO_POW2 rect when there is one); the diff lists dimension changes (`128x128 BGRA8888 to 64x64 BGRA8888`). A texture that fails to decode falls back to the strings file like any other file, and `snapshot take` lists every such path.

Texture decoding (`crates/dt-core/src/texture/decode.rs`) covers BGRA8888, RGBA8888, I8, IA88, DXT1, DXT5, ATI1N, ATI2N, BC7 and the PNG-payload formats, LZ4-compressed mips (COMPRESSED_MIP_SIZE) and sizes that are not multiples of four; cubemaps, volumes, arrays, JPEG and WebP payloads, BC6H, ETC and float formats are refused with a typed error. The format distribution of the real game's UI images is unknown until the first `--images all` snapshot on Windows; the `DEADTUNE_GAME_SAMPLES` test (section 19, GF-10) decodes every image in a snapshot and prints it. Vector icons rasterise through `texture::svg::rasterize` (resvg behind the `svg` feature, 591 KB on the Windows CLI; the GUI enables it) for previews.

"ConVar info" means what the files hold: the `ConVars` block of `gameinfo.gi` and the archived values in the cfg files. The engine's full ConVar list only exists in a running game (`cvarlist`), outside a snapshot.

On top of the categories, every snapshot writes `pak01.tsv`, one line per pak01 entry with its size and CRC. That is a few megabytes of text and lets a diff count what changed in the rest of the game (models, particles, maps) without copying any of it.

## Snapshot layout

```
<data dir>/game-files/<buildid>-<YYYYMMDD-HHMMSS>/
  manifest.toml        buildid, date, DeadTune version, options, one entry per file
  pak01.tsv            every pak01 entry: path, size, crc
  raw/<path>           bytes as in the game (pak paths as-is, loose files relative to game/citadel, the appmanifest under steam/)
  text/<path>          decoded: .vcss_c -> .css, .vxml_c -> .xml, .vjs_c -> .js, .vtex_c -> .png, .vsvg_c -> .svg
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
stored = "full"             # full | header | none (over the size cap)
text = "panorama/layout/citadel_hud_top_bar.xml"
decoded = "text"            # text | image | strings | none

[files.image]               # textures only
width = 128
height = 128
format = "BGRA8888"
mips = 1
```

The manifest header also records `images = "none" | "minimap_topbar" | "hud" | "all"`.

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
deadtune-cli snapshot take [--categories hud,settings,menu,panorama,deadtune,config] [--images none|minimap_topbar|hud|all] [--no-decode] [--size-cap <MB>]
deadtune-cli snapshot list
deadtune-cli snapshot diff <a> <b>      # folder names or build ids; "latest" and "previous" work too
deadtune-cli texture png <in.vtex_c> <out.png>
deadtune-cli texture svg <in.vsvg_c> <out.svg>
```

`take` prints the folder, file count, bytes and time, then every file that could not be decoded. `diff` writes the report into the newer snapshot and prints it. `texture png` decodes one compiled texture (the top mip) to a PNG and `texture svg` writes the source out of one vector icon, for looking at a single game file without a snapshot.

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
| Images | one nested scope (`ImageScope`) next to the categories, images exempt from the size cap, decoded to PNG and SVG beside the text | a seventh checkbox (3.3 GB with one tick, and no way to say "just the minimap"); per-folder globs (nothing to count or label) |
| Vector icons | resvg (measured 591 KB on the Windows CLI with default features off, behind the `svg` feature the GUI enables) | an in-house path rasteriser (no real `.vsvg_c` on the Mac to survey what the icons use; gradients, clip paths and transforms are likely) |

## Phases

1. Core: `snapshot::spec` (categories, rules, inventory), `snapshot::export` (take, manifest, decode, strings fallback, texture header), `snapshot::store` (folders, list, delete). Tests on a fake pak and a fake install. Done.
2. Core: `hud::kv3` legacy `VKV\x03` reader so the Wide FOV sample decodes; checked against the sample outside the repo (`DEADTUNE_KV3_SAMPLES`) and a hand-built fixture in the tests. Done.
3. Core: `snapshot::diff` with the DeadTune impact table, Markdown and TOML reports. Done.
4. CLI: `snapshot take | list | diff`. Done.
5. GUI: the page, the job, the setting, the auto snapshot on a game update, screenshots. Done.
6. Windows: take a snapshot of the real game, note size and time, open the decoded `popup_settings.xml`, and after the next update check the automatic snapshot and its report (`docs/testing-windows.md`, section 19).
7. Later, when the in-game settings work needs it: copy the real `popup_settings.vxml_c` from a snapshot into the fixtures.
8. UI images: core decoders, the image scope, PNG and SVG export, the CLI. Done on the Mac against the research mod textures (BGRA8888, DXT1, BC7, ATI1N) and synthetic blocks for the rest. Windows (GF-10): `--images all`, then the `DEADTUNE_GAME_SAMPLES` test over the folder to prove every real texture and icon decodes and to learn the format distribution and the SVG feature set.
9. GUI: an image scope picker on the Game files page and a browser that previews every UI image (`texture::thumbnail`, `texture::svg::rasterize`); then icon swapping through the write pipeline.

## Risks

- The real pak01 tree is large (tens of thousands of entries). Opening it reads only the header and tree; counting is a pass over a map. Measured on the fake install only (16 files, 3.0 MB stored, 0.4 s); the Windows check records the real numbers.
- A game update that changes the compiled resource header version or the KV3 version breaks decoding. The strings fallback keeps the raw bytes and the readable strings, and the diff still works on CRCs; only the text diff is lost. The manifest says `decoded = "strings"` so it is visible.
- Steam writes the new buildid at the end of an update, so the automatic snapshot should see complete files. If a pak chunk is still being written, a CRC error on that file is reported and the file is skipped; the next take fills it in.
- Snapshots are a few tens of megabytes each with Everything Panorama on. The page lists them with sizes and a Delete button; nothing is pruned automatically.
- `explorer`, `open` and `xdg-open` on a file open it with the default app. On Windows an `.xml` may open in a browser. Good enough for reading.
