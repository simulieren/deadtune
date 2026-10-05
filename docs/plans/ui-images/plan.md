# UI image overrides

Status: write side built (core, CLI, verify) and read side built (decoders, snapshot image scope, CLI export), both proven on the fake install and the research mod textures only; the GUI page is built (seen on the fake install and a 2706-file stand-in snapshot). In-game checks are `docs/testing-windows.md` sections 19 (GF-10), 20 and 21. Colour edits (tint, colorize, overlay, sliders, SVG palette swaps, bulk, undo) added 2026-10-05.

## Goal

Simon wants to swap any interface image in Deadlock for his own: minimap icons, the top bar, hero portraits, item icons. The game keeps them under `panorama/images/` in `pak01_dir.vpk`: 2706 files, of which 648 are vector images (`.vsvg_c`, 40 of them in the minimap and top bar) and the rest textures (`.vtex_c`). DeadTune never writes pak01. It ships replacements at the same paths in its own HUD addon, `addons/pak77_dir.vpk`, next to the layout, top bar and minimap edits.

The work splits into three parts.

| Part | What | State |
|---|---|---|
| Read | Decode any `.vtex_c` to RGBA (BGRA/RGBA/I8/IA88, BC1/BC3/BC4/BC5/BC7, PNG payloads, LZ4 mips), thumbnails, SVG rasterising, image export from snapshots and the CLI | built |
| Write | Turn the player's PNG or SVG into a game file at the original path and ship it safely | this plan, built |
| GUI | Browse the images, preview, drop a file on one, reset | built |

## Write side (built)

**Encoding.** `texture::encode::replace(original, image, fit)` rewrites a Panorama texture as uncompressed BGRA8888 with one mip and `NO_LOD`. The resource container is the game's: RED2, the reflectivity and the FALLBACK_BITS thumbnail stay; the COMPRESSED_MIP_SIZE table and the METADATA display rect go, because they describe the old pixels. This is the shape the Vindicta scope rebuild (`addons/native_scope.rs`) already proved the engine loads, including non-power-of-two sizes. The output is reparsed before it is returned.

`Fit::Original` (default) keeps the game image's size (its display rect when it has one) and letterboxes the player's image inside it with transparent padding. Cropping would cut off the player's art and stretching would distort it; transparent padding draws as nothing. Scaling is an area average in premultiplied alpha, so transparent pixels do not darken the edges. `Fit::Own` keeps the PNG's size, scaled down only past 4096 on the longer side.

Only single-mip `NO_LOD` 2D textures are accepted, which is how every `panorama/images/**` texture is compiled. Mipmapped textures, cubemaps, volumes, arrays and sprite sheets are refused with typed errors (`EncodeError`). World textures would need a generated mip chain; that is out of scope.

**Vector images.** `texture::svg::with_svg_text(original, svg)` writes SVG text into the game's `.vsvg_c`. The DATA layout (CRC, image table, text) is inferred from ValveResourceFormat's shared Panorama reader and has not been checked against a real file, so both reading and writing refuse a container whose text is not an SVG document. `svg::validate` checks the SVG is one well-formed element tree with an `<svg>` root and a positive `viewBox` or `width`/`height`.

A PNG for a vector path cannot become a texture: Panorama resolves `file://{images}/x.svg` to the `.vsvg_c`. `svg::png_in_svg` wraps the PNG as an SVG with an embedded `data:image/png` image sized to the game's view box. Whether Panorama's SVG renderer draws embedded rasters is unknown, so the override is marked experimental (`IconOverride::PngInSvg`) and Windows test IC-6 settles it.

**Model.** `HudLayout::icons` maps a game path to an `IconOverride`: a source plus an ordered list of colour adjustments, replayed on the source at every build.

```rust
pub struct IconOverride {
    #[serde(flatten)] pub source: Source,     // tagged `input`
    #[serde(default)] pub adjust: Vec<Adjust>, // skipped when empty
}
#[serde(tag = "input", rename_all = "snake_case")]
pub enum Source {
    Game,                                      // the game's own image, decoded at build time
    Png { image_sha256: String, fit: Fit },    // .vtex_c
    Svg { image_sha256: String },              // .vsvg_c
    PngInSvg { image_sha256: String },         // .vsvg_c, experimental
}
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Adjust {                              // texture::adjust
    Tint { color, strength }, Colorize { color, strength }, Overlay { color, blend, opacity },
    Hue { degrees }, Saturation { percent }, Brightness { percent }, Contrast { percent },
    Opacity { percent }, Invert, Swap { from, to },
}
```

Profiles from before adjustments carry only the `input` fields and load unchanged. An override whose source is `Game` with no adjustments is nothing, so the setters drop it from the map; a list has one slot per kind (tint, colorize and overlay share the colour slot; each swapped colour has its own), and an identity operation (strength 0, hue 0, opacity 100) removes its slot. `texture::adjust::apply_all` runs the list on an `RgbaImage` (alpha untouched except by `Opacity`); `texture::svg::adjust` maps every colour in an SVG's text through the same per-colour arithmetic (`adjust_rgb`), so a vector icon stays a vector: `svg::palette` lists the colours it names (fill, stroke, stop-color and friends as attributes, `style` attributes and `<style>` CSS; hex, `rgb()`, `rgba()` and the 148 CSS names; `currentColor` noted), `svg::map_colors`/`remap` rewrite only those tokens and leave every other byte alone. `Opacity` on an SVG multiplies the root's `opacity`.

It lives in the profile like every other HUD choice, so presets, Vanilla, ranked-safe and HUD removal cover it with no new paths. The image itself is copied to `<data>/icons/<sha256>.png|svg`, so the source file can be deleted. Stored images are never deleted on reset: another profile may use them. A `Game` source stores nothing: the profile holds only the recipe, and a game update that redraws the icon is picked up on the next apply.

**Building.** Every HUD build re-encodes each override from the player's current game file, so a game update that changes a header is picked up on the next apply, and the existing stale-on-buildid check prompts that apply. An override that cannot be built (the path is gone after an update, the stored image is missing or changed, the kind does not match the path) becomes an `IconProblem` on `HudPlan::icon_problems`; everything else ships. A plan whose only content was broken icons removes the addon.

**Safety.** Same pak, same rules: the addon is recorded by sha256 and a foreign file at our path is refused. The built pak is now read back through `addons::verify` against the game's files before `plan` returns, for every HUD feature: a texture must be one BGRA8888 `NO_LOD` level of exactly `width * height * 4` bytes with the game's RED2 block; a vector image must hold SVG text in the game's container. `addons verify` checks the installed pak the same way.

The launch guard (`addons/guard.rs`) covers the HUD addon too (`Pak::Hud`): a HUD pak that stops the game from starting goes back to the last verified HUD pak, or is removed.

**CLI.** `deadtune-cli hud icon list|set <game_path> <image> [--fit original|own]|reset <game_path>|reset-all --layout <hud.toml>`, then `hud apply --layout <hud.toml>`. `hud status --layout` lists replaced images; apply prints icons left out.

## Read side (built)

**Decoding.** `texture::decode::decode(bytes)` turns a `.vtex_c` into `RgbaImage` (the top mip, cropped to the FILL_TO_POW2 display rect when the header has one). One codec table covers the formats Panorama images compile to: BGRA8888, RGBA8888, I8, IA88, DXT1, DXT5, ATI1N, ATI2N, BC7, and the PNG-payload formats; mips listed in a COMPRESSED_MIP_SIZE table are LZ4 blocks (`crate::lz4`, now shared with the KV3 and Sinner readers); block formats with sizes that are not multiples of four decode the padded grid and clip. Cubemaps, volumes, arrays, JPEG and WebP payloads, BC6H, ETC and float formats are refused with `DecodeError::Unsupported`. `decode_mip(bytes, level)` reads one level, `thumbnail(bytes, max_side)` picks the smallest mip whose longer side still covers `max_side` and area-resamples the rest of the way, which is what a browser grid wants (Panorama images are single-mip, so it resamples the top). Proven on every `.vtex_c` in the research mod VPKs (BGRA8888 1080 scope, DXT1, BC7, ATI1N) and on hand-built blocks for the rest; the real game's format distribution waits for the first `--images all` snapshot (GF-10).

**Vector icons.** `svg::svg_text` reads the SVG out of the `.vsvg_c` container (the same guessed layout as the write side). `svg::rasterize(svg, max_side)` draws it with resvg (behind the `svg` cargo feature, which the GUI enables; 591 KB on the Windows CLI when measured) and returns straight-alpha RGBA, so the browser can show vector icons as pictures rather than text.

**Snapshots.** `snapshot::ImageScope` (`none`, `minimap_topbar`, `hud`, `all`) adds images to a snapshot next to the categories, exempt from the size cap; they decode to `text/<path>.png` and `.svg`. Details in `docs/plans/game-files/plan.md`, "UI images".

**CLI.** `deadtune-cli hud icon export <game_path> <out.png|out.svg>` writes the game's own image decoded; `deadtune-cli texture png <in.vtex_c> <out.png>` and `texture svg <in.vsvg_c> <out.svg>` do the same for a file on disk; `snapshot take --images hud` exports a whole scope.

## API for the GUI agent

```rust
// dt_core::texture (read side)
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, DecodeError>;              // top mip, display rect applied
pub fn decode_mip(bytes: &[u8], level: u8) -> Result<RgbaImage, DecodeError>;
pub fn thumbnail(bytes: &[u8], max_side: u32) -> Result<RgbaImage, DecodeError>;
impl RgbaImage { pub fn fit(&self, max_side: u32) -> RgbaImage; pub fn pixel(&self, x: u32, y: u32) -> [u8; 4]; }
pub fn svg::rasterize(svg: &str, max_side: u32) -> Result<RgbaImage, SvgError>;   // feature "svg"
pub struct snapshot::ImageInfo { width, height, format, mips }                     // from a texture header, no pixels

// dt_core::hud::icons
pub fn target(game_path: &str) -> Result<Target, IconError>;            // Raster | Vector, or why not
pub fn set(icons: &mut BTreeMap<String, IconOverride>, data_dir: &Path,
           game_path: &str, image: &[u8], fit: Fit) -> Result<IconOverride, IconError>;
pub fn set_fit(icons: &mut BTreeMap<String, IconOverride>, game_path: &str, fit: Fit) -> bool;
pub fn adjust(icons: &mut BTreeMap<String, IconOverride>, game_path: &str, adjust: Adjust) -> Result<bool, IconError>;
pub fn set_adjustments(icons: &mut BTreeMap<String, IconOverride>, game_path: &str, list: &[Adjust]) -> Result<bool, IconError>;
pub fn remove_adjust(icons: &mut BTreeMap<String, IconOverride>, game_path: &str, kind: AdjustKind) -> bool;
pub fn reset(icons: &mut BTreeMap<String, IconOverride>, game_path: &str) -> bool;
pub fn reset_all(icons: &mut BTreeMap<String, IconOverride>);
impl IconOverride {
    pub fn image_sha256(&self) -> Option<&str>;                        // None for a Game source
    pub fn is_experimental(&self) -> bool;
    pub fn fit(&self) -> Option<Fit>;
    pub fn stored_at(&self, data_dir: &Path) -> Option<PathBuf>;       // the player's copy, for previews
}

// dt_core::texture::adjust and texture::svg (colour edits)
pub fn adjust::apply_all(image: &mut RgbaImage, list: &[Adjust]);
pub fn adjust::adjust_rgb(rgb: [u8; 3], adjust: &Adjust) -> [u8; 3];
pub fn adjust::set(list: &mut Vec<Adjust>, adjust: Adjust) -> bool;     // one slot per kind
impl Adjust { pub fn parse(spec: &str) -> Result<Adjust, String>; pub fn label(&self) -> String; }
pub fn svg::palette(svg: &str) -> Palette;                                 // Swatch { rgb, uses }, current_color
pub fn svg::remap(svg: &str, map: &BTreeMap<Rgb, Rgb>) -> String;
pub fn svg::adjust(svg: &str, list: &[Adjust]) -> String;
pub struct IconProblem { pub game_path: String, pub reason: String }   // HudPlan::icon_problems

// dt_core::texture
pub fn encode::replace(original: &[u8], image: &RgbaImage, fit: Fit) -> Result<Vec<u8>, EncodeError>;
pub fn svg::with_svg_text(original: &[u8], svg: &str) -> Result<Vec<u8>, SvgError>;
pub fn svg::svg_text(compiled: &[u8]) -> Result<String, SvgError>;
pub fn png::read(bytes: &[u8]) -> Result<RgbaImage, PngError>;
pub fn png::write(image: &RgbaImage) -> Result<Vec<u8>, PngError>;
```

`data_dir` is `BackupStore::root`, the same directory `apply::hud_plan` passes to the HUD build. Pass `&mut state.profile.hud.icons`; Apply does the rest.

## GUI (built)

`crates/dt-gui/src/images.rs` holds the page state and every transition (source, folders, filters, selection, drop, fit, reset, export) with tests; `thumbs.rs` decodes pictures on three worker threads (each frame's visible tiles replace the queue, so tiles scrolled past never decode) into egui textures in a 96 MB least-recently-used cache; `images_view.rs` draws it. Exports go to `<data>/exports/<folder>/<name>.png|svg` (no file dialog crate: drag-and-drop and a path box instead). Vector icons accept a PNG, labelled as a test, until IC-6 settles it. Icon problems from the current plan show as a red dot on the tile and a sentence in the panel.

**Colour edits** (`images_edit.rs`, `images_edit_view.rs`): the panel's "Edit colours" section edits the selected override's adjustment list. Slider moves go into a draft that the live preview renders (on the thumbnail workers, off the UI thread) and that is committed into the profile once it has sat still for 250 ms or the drag ends, because every profile change replans the whole HUD pak. Every page edit (drop, slider commit, remove, bulk, reset) pushes a snapshot of `profile.hud.icons` onto an undo stack for the session (Ctrl+Z, Ctrl+Shift+Z, buttons); Discard changes clears it. Bulk applies one adjustment to the visible set (folder and search) or to Ctrl/Shift-marked tiles, each image keeping its own override; "Reset this folder" resets the visible set. For SVGs the panel lists the icon's palette with a picker per colour (a `Swap` each). Windows checks: `docs/testing-windows.md` section 22.

The original brief:

- **Browser.** A grid of every `panorama/images/**` entry from pak01's tree (instant, no pixels read), grouped by folder with the minimap, top bar and hero portraits first, and a search box. Thumbnails decode lazily through `texture::thumbnail` (textures) and `svg::rasterize` (vector icons) on the job thread, a few per frame. Overridden tiles carry a badge; experimental ones say so.
- **Preview.** Selecting a tile shows the game image and the player's image side by side at the game's size, with the fit toggle (Original letterboxed, Own size) where it applies.
- **Drop a file.** eframe reports dropped files (`ctx.input(|i| i.raw.dropped_files.clone())`). A PNG or SVG dropped on a tile calls `icons::set`; errors show as one sentence under the tile (`IconError`'s text). A file picker button does the same.
- **Reset.** A Reset button per overridden tile and a Reset all button for the page.
- **Problems.** After planning, list `HudPlan::icon_problems` in plain words with a Reset action each.
- **Vector icons.** Offer SVG only until IC-6 shows Panorama draws a PNG inside an SVG; then offer PNG too, still labelled experimental until it has shipped a while.
- **Credit.** None needed: the art is the player's own.

## Unverified until Windows

- The engine draws a BGRA8888 replacement for a texture that was BC-compressed with a display rect (only an already-BGRA texture, the scope, has been proven).
- `Fit::Own` sizes draw at the CSS size, not the texture's.
- The `.vsvg_c` DATA layout guess, and that Panorama renders our SVG.
- Panorama drawing an embedded PNG inside an SVG.
