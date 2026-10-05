# UI image overrides

Status: write side built (core, CLI, verify), proven on the fake install only; the read side is being built in parallel; the GUI page is not started. In-game checks are `docs/testing-windows.md` section 20. Last update 2026-10-05.

## Goal

Simon wants to swap any interface image in Deadlock for his own: minimap icons, the top bar, hero portraits, item icons. The game keeps them under `panorama/images/` in `pak01_dir.vpk`: 2706 files, of which 648 are vector images (`.vsvg_c`, 40 of them in the minimap and top bar) and the rest textures (`.vtex_c`). DeadTune never writes pak01. It ships replacements at the same paths in its own HUD addon, `addons/pak77_dir.vpk`, next to the layout, top bar and minimap edits.

The work splits into three parts.

| Part | What | State |
|---|---|---|
| Read | Decode any `.vtex_c` to RGBA (every BC format), `texture/png.rs` read and write, image export from snapshots | parallel agent |
| Write | Turn the player's PNG or SVG into a game file at the original path and ship it safely | this plan, built |
| GUI | Browse the images, preview, drop a file on one, reset | not started |

## Write side (built)

**Encoding.** `texture::encode::replace(original, image, fit)` rewrites a Panorama texture as uncompressed BGRA8888 with one mip and `NO_LOD`. The resource container is the game's: RED2, the reflectivity and the FALLBACK_BITS thumbnail stay; the COMPRESSED_MIP_SIZE table and the METADATA display rect go, because they describe the old pixels. This is the shape the Vindicta scope rebuild (`addons/native_scope.rs`) already proved the engine loads, including non-power-of-two sizes. The output is reparsed before it is returned.

`Fit::Original` (default) keeps the game image's size (its display rect when it has one) and letterboxes the player's image inside it with transparent padding. Cropping would cut off the player's art and stretching would distort it; transparent padding draws as nothing. Scaling is an area average in premultiplied alpha, so transparent pixels do not darken the edges. `Fit::Own` keeps the PNG's size, scaled down only past 4096 on the longer side.

Only single-mip `NO_LOD` 2D textures are accepted, which is how every `panorama/images/**` texture is compiled. Mipmapped textures, cubemaps, volumes, arrays and sprite sheets are refused with typed errors (`EncodeError`). World textures would need a generated mip chain; that is out of scope.

**Vector images.** `texture::svg::with_svg_text(original, svg)` writes SVG text into the game's `.vsvg_c`. The DATA layout (CRC, image table, text) is inferred from ValveResourceFormat's shared Panorama reader and has not been checked against a real file, so both reading and writing refuse a container whose text is not an SVG document. `svg::validate` checks the SVG is one well-formed element tree with an `<svg>` root and a positive `viewBox` or `width`/`height`.

A PNG for a vector path cannot become a texture: Panorama resolves `file://{images}/x.svg` to the `.vsvg_c`. `svg::png_in_svg` wraps the PNG as an SVG with an embedded `data:image/png` image sized to the game's view box. Whether Panorama's SVG renderer draws embedded rasters is unknown, so the override is marked experimental (`IconOverride::PngInSvg`) and Windows test IC-6 settles it.

**Model.** `HudLayout::icons` maps a game path to an `IconOverride`:

```rust
#[serde(tag = "input", rename_all = "snake_case")]
pub enum IconOverride {
    Png { image_sha256: String, fit: Fit },   // .vtex_c
    Svg { image_sha256: String },             // .vsvg_c
    PngInSvg { image_sha256: String },        // .vsvg_c, experimental
}
```

It lives in the profile like every other HUD choice, so presets, Vanilla, ranked-safe and HUD removal cover it with no new paths. The image itself is copied to `<data>/icons/<sha256>.png|svg`, so the source file can be deleted. Stored images are never deleted on reset: another profile may use them.

**Building.** Every HUD build re-encodes each override from the player's current game file, so a game update that changes a header is picked up on the next apply, and the existing stale-on-buildid check prompts that apply. An override that cannot be built (the path is gone after an update, the stored image is missing or changed, the kind does not match the path) becomes an `IconProblem` on `HudPlan::icon_problems`; everything else ships. A plan whose only content was broken icons removes the addon.

**Safety.** Same pak, same rules: the addon is recorded by sha256 and a foreign file at our path is refused. The built pak is now read back through `addons::verify` against the game's files before `plan` returns, for every HUD feature: a texture must be one BGRA8888 `NO_LOD` level of exactly `width * height * 4` bytes with the game's RED2 block; a vector image must hold SVG text in the game's container. `addons verify` checks the installed pak the same way.

The launch guard (`addons/guard.rs`) covers the HUD addon too (`Pak::Hud`): a HUD pak that stops the game from starting goes back to the last verified HUD pak, or is removed.

**CLI.** `deadtune-cli hud icon list|set <game_path> <image> [--fit original|own]|reset <game_path>|reset-all --layout <hud.toml>`, then `hud apply --layout <hud.toml>`. `hud status --layout` lists replaced images; apply prints icons left out.

## API for the GUI agent

```rust
// dt_core::hud::icons
pub fn target(game_path: &str) -> Result<Target, IconError>;            // Raster | Vector, or why not
pub fn set(icons: &mut BTreeMap<String, IconOverride>, data_dir: &Path,
           game_path: &str, image: &[u8], fit: Fit) -> Result<IconOverride, IconError>;
pub fn reset(icons: &mut BTreeMap<String, IconOverride>, game_path: &str) -> bool;
pub fn reset_all(icons: &mut BTreeMap<String, IconOverride>);
impl IconOverride {
    pub fn image_sha256(&self) -> &str;
    pub fn is_experimental(&self) -> bool;
    pub fn stored_at(&self, data_dir: &Path) -> PathBuf;               // the player's copy, for previews
}
pub struct IconProblem { pub game_path: String, pub reason: String }   // HudPlan::icon_problems

// dt_core::texture
pub fn encode::replace(original: &[u8], image: &RgbaImage, fit: Fit) -> Result<Vec<u8>, EncodeError>;
pub fn svg::with_svg_text(original: &[u8], svg: &str) -> Result<Vec<u8>, SvgError>;
pub fn svg::svg_text(compiled: &[u8]) -> Result<String, SvgError>;
pub fn png::read(bytes: &[u8]) -> Result<RgbaImage, PngError>;
pub fn png::write(image: &RgbaImage) -> Result<Vec<u8>, PngError>;
```

`data_dir` is `BackupStore::root`, the same directory `apply::hud_plan` passes to the HUD build. Pass `&mut state.profile.hud.icons`; Apply does the rest.

## GUI (to build)

- **Browser.** A grid of every `panorama/images/**` entry from pak01's tree (instant, no pixels read), grouped by folder with the minimap, top bar and hero portraits first, and a search box. Thumbnails decode lazily through the read side's `texture::decode`; vector images show their SVG as text or a placeholder until there is a rasteriser. Overridden tiles carry a badge; experimental ones say so.
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
