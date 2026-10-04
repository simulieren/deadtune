# Deadlock HUD modding: VPK + Panorama compiled resource research

Scratchpad root: `/private/tmp/claude-501/-Users-simonkhalimonov-Projects-Code-deadlock-config-editor/355ca0a6-c88e-49ea-a19f-3f89ba30d7e5/scratchpad`
(called `$S` below).

Markers: **[V]** verified this session by running code or reading bytes. **[I]** inferred from verified facts. **[U]** unverified or unknown. Nothing here was tested in the running game (no Deadlock install available), so "the game accepts X" is always [I] or [U].

## 1. Inputs and VPK inventory

Unzipped to `$S/vpk/`. `_incoming/vpk.zip` holds `deadlock-config-research/configs/...` (7 VPKs). `_incoming/gamebanana.zip` only contains the three GameBanana zips again (qollite, qollock, optilock_fps_configs), so `research/gamebanana/qollock.zip` and `qollite.zip` are the same files. [V]

Nine VPKs total, all parsed with `$S/vpkr.py` (own reader) and cross-checked with Source2Viewer-CLI 20.0 (`--vpk_list`, `--vpk_verify`). Full listings: `$S/listing.txt`, `$S/qollock_files.txt`.

| VPK | Entries | HUD relevant? |
|---|---|---|
| `qollite/pak01_dir.vpk` (17.4 MB) | 184 | Yes. 60 vcss, 34 vxml, 48 vjs, minimap images |
| `qollock/pak03_dir.vpk` (27.8 MB) | 2250 | Yes. 156 vcss, 25 vxml, ~248 vjs (framework), images. Also junk (see 1.2) |
| `OptimizationLock/.../Blur Disabler/pak97_dir.vpk` | 2 | Yes, but only blur defines (see 3.4) |
| `.../Vindicta Scope Downscale/pak89_dir.vpk` | 2 | Crosshair scope image `panorama/images/hud/crosshair/scope_common_psd.vtex_c` + `panorama/image_compiler.vdata_c` |
| `.../Screenspace Particle Disabler/pak02_dir.vpk` | 109 | No panorama |
| `.../Optimized Soul Container`, `Sinner Light Fix Mod/pak26`, `OptiLock/Essential Fixes/*` | 2-23 | No panorama |

Mod VPK file names: `pakNN_dir.vpk` with NN = 01, 02, 03, 26, 89, 97 in these samples. [V]

### 1.1 Panorama file types seen (all with `_c` suffix)

`.vcss_c` (styles), `.vxml_c` (layouts), `.vjs_c` (scripts), `.vtex_c` (images), `.vsvg_c`, `.vdata_c` (`panorama/image_compiler.vdata_c`, present in 3 mods; purpose [U], probably the image-atlas/compiler manifest). [V]

### 1.2 Oddities in qollock

- 991 entries live under `scripts/game_update_runs/gametracking-deadlock/game/citadel/pak01_dir/panorama/{styles,layout,scripts}/...`. These are recompiled copies of vanilla files, compiled with search path `citadel_addons/qollock-codex` (see RED2 in 2.3). They are not at the mounted path so they are inert, but they give a vanilla text baseline that I used for diffs. [V]
- ~1200 `node_modules/...` and `eslint.config.vjs_c` entries (packaging accident). [V]
- `panorama/scripts/core/ql_storage_bridge.vjs_c` stores settings through a hidden CEF HTML panel loading `https://predi-i.github.io/qollock-updates/bridge.html` (localStorage). [V, from decompiled JS]. Not something DeadTune should copy.

## 2. Compiled resource binary format (Source 2, header version 12)

Observed on 721+ `.vcss_c` files (every one parsed has block layout RED2, DATA, SrMa; header version 12; resource version 3), plus 3 `.vxml_c` and 2 `.vjs_c` samples. [V]

### 2.1 File header (offsets in bytes, little endian)

Sample: `$S/raw/hud_abilities_mod.vcss_c` (1581 bytes, extracted from `qollite/pak01_dir.vpk` entry `panorama/styles/hud_abilities.vcss_c`, VPK data offset 592077).

| Offset | Size | Value in sample | Meaning |
|---|---|---|---|
| 0x00 | u32 | 0x62d = 1581 | file size (equals actual size, checked on all samples) |
| 0x04 | u16 | 12 | header version |
| 0x06 | u16 | 3 (vcss, vxml) or 4 (vjs) | resource type version (CLI prints `PanoramaStyle [Version 3]`, `PanoramaScript [Version 4]`, `PanoramaLayout [Version 3]`) |
| 0x08 | u32 | 8 | block table offset, relative to this field, so table starts at 0x10 |
| 0x0C | u32 | 3 | block count |
| 0x10 + 12*i | char[4], u32, u32 | `RED2`,`DATA`,`SrMa` | block name, offset **relative to the offset field itself** (entry+4), size |

Worked example: entry 1 is at 0x1C, name `DATA`, offset field at 0x20 = 0x330, so absolute = 0x20+0x330 = 0x350 = 848, size 0x174 = 372. [V]

Layout rules observed: blocks are placed in table order, each starts 16-byte aligned (padding zero), the last block is not padded, and file size = end of last block. RED2 starts at 64 for 3 blocks (table ends at 52), at 48 for 2 blocks. [V: my rebuild script reproduces the original file byte for byte, `cmp` identical, for hud_abilities_mod.vcss_c and hud_mod.vcss_c.]

### 2.2 Block types per resource type

| Ext | Blocks | Plain text? |
|---|---|---|
| `.vcss_c` | RED2, DATA, SrMa | DATA holds the CSS as plain UTF-8, **minified** (`{`, `;` without whitespace, `@import`/`@define` kept). Not compressed. |
| `.vjs_c` | RED2, DATA | DATA is plain minified JavaScript, no prefix (first bytes `(function(){`). |
| `.vxml_c` | RED2, DATA (6 bytes), LaCo | **No plain XML.** LaCo is a KeyValues3 binary AST (`m_AST.m_pRoot ... eType="STYLES"/"INCLUDE"/...`). |

`RED2` (replaces the older `REDI`) and `SrMa` (source map) are KV3 v5 binary blobs: magic bytes `05 33 56 4B` ("\x053VK"), format GUID `7c161274-e906-9846-aff2-e63eb59037e7` (generic), at 0x40 in the sample. The `LaCo` block uses the same magic with compression method field = 1 (LZ4, [I] from KV3 v5 knowledge, value 1 verified). [V]

### 2.3 RED2 contents (what the compiler recorded)

Decoded with `Source2Viewer-CLI -b RED2`. For the sample:

```
m_InputDependencies = [
  { m_RelativeFilename="panorama/styles/hud_abilities.css", m_SearchPath="citadel_addons/qollite",
    m_nFileCRC=3333193841, m_bOptional=false, m_bFileExists=true, m_bIsGameFile=false },
  { m_RelativeFilename="panorama/styles/hud_abilities.vcss", m_nFileCRC=0, m_bOptional=true, m_bFileExists=false } ]
m_SpecialDependencies = [ "Panorama Preprocessor Presence/Version" fp 0, "Panorama Style Compiler Version" fp 9 ]
```

So both mods were compiled with Valve's resourcecompiler from an addon content dir `citadel_addons/<modname>`. The vjs compiler version fingerprint is 4 ("JavaScript Compiler Version"). [V]

### 2.4 Checksums that matter if we generate files

1. **Resource-level DATA prefix (vcss/vxml).** `DATA[0:4]` is a CRC-like u32, `DATA[4:6]` is u16 = 0 (Names count in VRF terms), text starts at `DATA[6:]`. For all 4 `.vcss_c` I tested (hud.vcss_c, hud_abilities x3):
   `DATA[0:4] == m_nFileCRC(source .css, from RED2) XOR crc32(DATA text)`. [V, 4/4]
   For `.vxml_c` the DATA is 6 bytes (empty text, crc32 = 0), so the u32 equals the source `.xml` `m_nFileCRC` exactly (0x8c047de9 = 2349104617 for hud.xml). [V, 1/1]
   For `.vjs_c` there is no prefix at all. [V]
   Generation trick (works without knowing the source CRC): take the template's `m_nFileCRC = old_prefix ^ crc32(old_text)`, then `new_prefix = m_nFileCRC ^ crc32(new_text)`. [V that it round-trips on unchanged text; that the engine checks it is [U]]
2. **Source Map (SrMa)** holds `DBITSLC` arrays of [text offset, line, column] triples. It maps minified positions to source lines. If you change the text and keep the old SrMa the map is stale but structurally valid. [V that VRF parses it; whether the engine reads it at load: U]
3. **File size** (header u32) and **block offsets/sizes**: recompute (trivial). [V]
4. **No CRC over the whole file or over RED2.** I searched every contiguous sub-range of the DATA block with crc32, crc32c and adler32 and none matched the prefix except through the XOR relation above. [V]
5. VRF behaviour as a proxy (not the game): with SrMa present VRF accepts any prefix; when I dropped SrMa, VRF raised `CRC32 mismatch for read data` unless prefix == crc32(text) exactly. [V] This hints that Valve's own reader treats the two cases differently; I never saw a vcss_c without SrMa in the wild (0 of 722). So **always keep the SrMa block** when generating.

### 2.5 Can we generate a valid `.vcss_c` from plain CSS?

Method (script: `$S/gen/mkvcss.py`):
1. Take a real `.vcss_c` as a template (best: the user's own installed vanilla file from game `pak01_dir.vpk`, or any compiled file of the same type).
2. Keep RED2 and SrMa bytes.
3. Replace DATA with `u32 prefix | u16 0 | minified css text` where prefix = templateSourceCRC ^ crc32(text).
4. Rewrite header file size, block table offsets and sizes, 16-byte alignment.

Results [V]:
- Unchanged text reproduces the original byte-identical.
- Generated files (`$S/gen/test_keep.vcss_c`, `$S/gen/hud_new_keep.vcss_c`, 119049 bytes with an appended `#minimap_persp{width:300px;height:300px;}`) parse in Source2Viewer-CLI, `-b DATA` prints the new text, `-d` decompiles to a `.css` that contains the new rule, and they survive packing into a VPK (`$S/gen/pak99_dir.vpk`, `--vpk_verify` prints Success).

Not verified (the key uncertainties):
- **Engine acceptance.** The game is the only real test. [U]
- Whether the engine validates the DATA prefix against the text. [U] We produce the same value the Valve compiler produces, so it passes either way, as long as the formula is right beyond 4 samples. Sample is small (4 files); [I] that it holds generally.
- Whether the engine cares that RED2 names a different source file or search path than reality. [U, I that it does not: all mods carry foreign paths like `citadel_addons/qollite` and work for their users]
- Whether the CSS text must be in the compiler's normalised form. Our test text is minified like the compiler's, which is the safest. If the user's CSS contains `@define` variables or `@import` of `.vcss` (non-_c), the Valve preprocessor rewrote them to `s2r://...vcss_c` and expanded things; we must write already-preprocessed text. [I]
- Compiler fingerprints in RED2 (Style Compiler Version = 9). A game update that bumps this could make old compiled files rejected or recompiled. [U] Cloning the template from the user's current install keeps RED2 in sync with the installed game build. [I]

**`.vjs_c`**: same approach but simpler (DATA = text, no prefix). [I, not built]. **`.vxml_c`**: requires emitting a KV3 v5 binary LaCo block (LZ4) with an AST; not feasible without a KV3 writer, and getting it wrong breaks the whole HUD. Recommendation: do not generate vxml_c; avoid layout changes. [I]

## 3. How the mods override HUD

### 3.1 Stub plus `base/` copy pattern (both mods, plus Blur Disabler)

A mod cannot append to a vanilla stylesheet; VPK override replaces the entire file at the same path. Both mods therefore ship:
- `panorama/styles/<name>.vcss_c`: a stub starting with `@import url("s2r://panorama/styles/base/<name>.vcss_c");` followed by override rules.
- `panorama/styles/base/<name>.vcss_c`: a full copy of the original stylesheet.

Verified examples: qollock `hud.css` is a 24-line stub that imports `base/hud.vcss_c` then ~20 `features/ql_feat_*.vcss_c` files; qollite `hud.css` imports base and adds rules; Blur Disabler `citadel_base_styles.css` is 2 defines (below). [V]

Downside: the `base/` copy goes stale on every game update (qollite's `base/hud.css` still differs from the current vanilla by dozens of lines vs the vanilla text in qollock's baseline). [V by diff]

**Better approach for DeadTune [I]:** read the installed game's own compiled `hud.vcss_c`, take its DATA text, append our minified rules, and write the result as the override at the original path. No `base/` copy, no `@import`, no staleness (regenerate when `buildid` changes). Appending works if later rules win at equal specificity, which is how the mods' stubs behave after `@import`. The rebuild test in 2.5 did exactly this on qollite's hud.vcss_c text.

### 3.2 Layout and script hooks (qollite and qollock)

Both replace `panorama/layout/hud.vxml_c` and `citadel_hud_top_bar.vxml_c`, to (a) add `<scripts><include src="s2r://panorama/scripts/....vjs_c"/></scripts>`, (b) add new panels. Example qollite `hud.vxml` diff: wraps the minimap in `<Panel id="minimap_persp_wrapper">`, adds `#minimap_overlay_root`, `#minimap_markers`, `#minimap_urn_host`, and ~20 `qollite_map_*.js` includes. Top bar adds `#UrnTracker`, `#BuffHUD`, `#RejuvHUD`, `<scripts>qollite_topbar.vjs_c`. Qollock includes ~60 `core/ql_*.vjs_c` and `manifests/ql_*/manifest.vjs_c` files. [V]

Alternative to restyling by CSS: JS at runtime does `root.FindChildTraverse("TopBar").style.x = ...`, `.style.uiScale`, `.style.opacity`, `.style.washColor`. That needs a modified `hud.vxml_c` (LaCo), which we can't generate. CSS-only avoids this. [I]

### 3.3 HUD map: panel IDs, files, and edits

Vanilla IDs from `$S/dec/van/layout/hud.xml` (decompiled) and `$S/dec/van/styles/hud.css`. Styles live mostly in `hud.vcss_c`, others as listed.

| HUD element | Panel id / class | Style file | Vanilla values [V] |
|---|---|---|---|
| Minimap (whole) | `#minimap_persp` (GlobalClassListener, classes `gDetailView gScoreboardOpen`) | `hud.vcss_c` (+ `hud_minimap.vcss_c` for icons/markers) | `width:440px; height:520px; margin-bottom:15px; vertical-align:bottom; horizontal-align:right; perspective:900; transition-property:width,height` |
| Minimap inner | `#minimap_container`, `#minimap_frame`, `#HudMinimapContainer`, `#hud_minimap` | `hud.vcss_c` | container `margin-top:80px; width/height:400px; ui-scale:105%`; frame 400x400 with `world-blur: ingameHudBlur`; `#HudMinimapContainer` 105%x105% |
| Minimap sizes | `@define HudMinimapSize-1..6` (665px ... 1662.5px) | `hud_minimap.vcss_c` | |
| Top bar | `CitadelHudTopBar` (id `TopBar`), `#GradientBacker`, `.GameTime`, `.TeamScore`, `#TeamsContainer` > `CitadelHudTopBarTeam` (`#TeamFriendly`, `#TeamEnemy`), `#ObjectivesMap`, `#RejuvenatorCharges`, `#ScoreCounter` | `citadel_hud_top_bar.vcss_c`, `citadel_hud_top_bar_player.vcss_c`, `citadel_hud_top_bar_team.vcss_c`, `citadel_hud_top_bar_chat.vcss_c`, `topbar_rank_*` (qollite) | root: `height:100%; width:fit-children; vertical-align:top; horizontal-align:center; z-index:10` |
| Health, ammo cluster | `#health_and_abilities_container` (`CitadelHudHealthContainer`), `#hud_health_bars`, `#HealthBarContent`, `#ammo_panel`, `#clip_status` | `hud.vcss_c`, `hud_health.vcss_c`, `hud_health_container.vcss_c` | `width:250px; height:380px; vertical-align:bottom; horizontal-align:center; margin-left:300px; margin-right:1290px; margin-bottom:20px; flow-children:none; overflow:noclip` |
| Signature ability slots (4) | `#hud_signature` (`slots="signature_1..4"`), icons `.ability_container` | `hud.vcss_c`, `hud_abilities.vcss_c`, `hud_ability_icon.vcss_c`, `hud_ability_icon_active.vcss_c` | `#hud_signature`: `visibility:collapse; horizontal-align:center; vertical-align:bottom; margin-top:5px; ui-scale:90%` |
| Item active slots (4) | `#ActiveAbilitiesMenu` (`slots="item_1..item_4"`), `#CosmeticAbilitiesMenu`, `#HeldAbilitiesMenu` | `hud.vcss_c` | `#ActiveAbilitiesMenu {horizontal-align:right; margin-right:100px}`; `#CosmeticAbilitiesMenu {margin-right:430px; opacity:0.3}` |
| Passive item slots | `#hud_passive_items` | `hud.vcss_c`, `hud_ability_icon_passive.vcss_c` | `height:60px; width:600px; margin:20px; visibility:collapse; vertical-align:top; ui-scale:130%` |
| Abilities container | `#AbilitiesContainer.abilitiesContainer` | `hud.vcss_c` | `horizontal-align:center; vertical-align:bottom; width:1140px; height:420px; overflow:noclip` |
| Bought items / stats (lower left) | `#StatsAndModsContainer` > `#LowerLeft` > `#hudPlayerStats`, `#gold_and_ap_container`, `#ModsContainer`, `#CitadelHudQuickbuy` | `hud.vcss_c`, `citadel_hud_item_bar_graph.vcss_c`, `hud_quickbuy.vcss_c` | `#LowerLeft {horizontal-align:left; vertical-align:bottom; padding-top:20px; height:100%}` |
| Crosshair | `Citadel_AbilityHUDPanels` id `crosshair` | `ability_hud_elements/*.vcss_c` (`element_gun`, ...) | |
| Kill feed / events | `HudDataFeed` id `DataFeed`, `CitadelHudEventIndicatorsPanel`, `CitadelHudCombatLog`, `CitadelChat` id `Chat` | `hud_event_indicator.vcss_c`, `citadel_hud_data_feed_*`, `hud_damage_report.vcss_c` | |
| Soul / AP counters | `#gold_and_ap_container`, `#APContainer` | `hud_gold_and_ap_container.vcss_c` | |

Class hooks usable from CSS without JS: `.AspectRatio4x3`, `.AspectRatio16x10`, `.AspectRatio21x9`, `.gShopOpen`, `.gScoreboardOpen`, `.gDetailView`, `.GameStatePreGame`, `.viewing_as_player`, `.no_minimap`, `.useZoomedMinimap`. [V in vanilla css]

### 3.4 Actual rules the mods use

Decompiled outputs: `$S/dec/qollite/panorama/...`, `$S/dec/qollock/panorama/...`, vanilla baseline `$S/dec/van/...`.

**Qollite `hud.css` (diff vs vanilla, 222 changed lines)** [V]:

```css
@import url("s2r://panorama/styles/base/hud.vcss_c");
#hud_passive_items { height:60px; width:600px; margin:20px; margin-top:-6%;
  visibility:visible; horizontal-align:center; vertical-align:top; ui-scale:105%; }  /* show + move passive slots */
.AspectRatio4x3 #health_and_abilities_container { horizontal-align:center; margin-right:962px; margin-left:200px; }
.gDetailView #minimap_persp,.gScoreboardOpen #minimap_persp { width:800px; height:800px;
  vertical-align:center; horizontal-align:center; opacity:.5; }   /* big map on TAB */
.gDetailView #minimap_container,.gScoreboardOpen #minimap_container { width:800px; height:800px; }
#minimap_persp.DisableBigMapScaleOnTab.gDetailView, ... { width:500px; height:500px;
  vertical-align:bottom; horizontal-align:right; opacity:1; }
.AspectRatio4x3 #StatsAndModsContainer.gShopOpen #LowerLeft { transform: translateX(10px) translateY(-10px); }
#minimap_ui_clamp_container, #minimap_persp { overflow:noclip; }
#HudMinimapContainer { z-index:900; }
```
Changed vanilla lines: `background-color:#999999` -> `none` and `border:4px solid white` -> `0px` (some frame), `margin-top:0px` -> `110px`, two `visibility:collapse` -> `visible`.

**Qollite `hud_minimap.css` (+675 lines)**: wrapper `#minimap_persp_wrapper {width/height:fit-children; vertical-align:bottom; horizontal-align:right; flow-children:down; margin-bottom:15px; overflow:noclip}`; `#minimap_persp_wrapper #minimap_persp {width:400px; height:400px}`; `#hud_minimap.BmMinimalMap {background-color: rgba(0,0,0,0); border-radius:0px}`; per-size classes `#hud_minimap.bm_size_self_50 .map_button.player.friend.localplayer { pre-transform-scale2d: 0.5; }` (50-160). Class names are set by JS from user settings. [V]

**Qollite `hud_abilities.css`**: shows hidden passives: `.ASAPOn .items .ability_container.item_passive.Hidden{visibility: visible;}` and `#gameplay_hud.UMM_ShowPassives ... {visibility: visible;}`. **`hud_ability_icon.css`**: same extra rule. [V]

**Qollite `citadel_hud_top_bar.css`**: removes the `.connectedToHideout ... {visibility:collapse}` rules, adds `margin-top:0px`, new soul/SPM labels (`.playerSPMDisplay`, `.teamSPMDisplay`, `.SpentSoulDisplay`, `.UnSoulsValueContainer {margin-top:2px; visibility:visible}`). [V]

**Qollock `features/ql_feat_minimap.css`**: [V]
```css
#minimap_container { width:400px; height:400px; margin-right:15px; transform:none; }
#minimap_frame { width:400px; height:400px; }
.minimalist_minimap_active #minimap_frame { visibility:collapse; }
.minimalist_minimap_active #HudMinimapContainer { background-color: rgba(0,0,0,0); world-blur:none; }
#minimap_persp { transform-origin:100% 100%; transition-property: opacity, margin, margin-left, margin-right, margin-top, margin-bottom, pre-transform-scale2d; transition-duration:.12s; }
```
**Qollock `ql_feat_aspect_ratio.css`** (16:10 and 4:3 support, shows "hide/move" patterns): [V]
```css
.support_4_3_active #health_and_abilities_container { margin-right:960px; }
.support_16_10_active .AspectRatio16x10 #health_and_abilities_container { horizontal-align:center; margin-right:1030px; margin-left:0px; transform:translateX(6%) translateY(7%); }
.support_16_10_active #health_and_abilities_container { ui-scale:104%; }
.support_16_10_active #hudPlayerStats { vertical-align:bottom; horizontal-align:left; flow-children:down; margin-bottom:270px; width:280px; transform:translateY(7%) translateX(-1.1%); }
.support_16_10_active .ModsContainer { horizontal-align:left; vertical-align:bottom; height:102px; width:380px; margin-bottom:16px; margin-left:16px; flow-children:right; ui-scale:90%; }
```
**Qollock `ql_feat_healthbar.css`**: `.minimalist_healthbar_active #health_and_abilities_container { width:974px; transform: translateX(0px) translateY(-15px); margin-right:20px; margin-left:20px; }`, `.fg_healthbar_active ... { width:400px; height:300px; vertical-align:bottom; horizontal-align:center; ui-scale:120%; ... }`. [V]
**Qollock `ql_feat_ability_icons.css`**: `.simplify_ability_icons_active #CosmeticAbilitiesMenu { visibility:collapse; }`. **`ql_feat_stats_position.css`**: `#hudPlayerStats.QolStatsRight { horizontal-align:right; width:fit-children; }`. [V]

**Qollock JS runtime positioning** (the mod's "offset X/Y/scale/opacity" sliders), generic pattern in `manifests/ql_topbar`, `ql_bottom_bar`, `ql_items`, `ql_stats_position`, `ql_ammo`: [V]
```js
topBar.style.x = ox + "px"; topBar.style.y = (-oy) + "px"; topBar.style.uiScale = Math.round(sc*100) + "%"; topBar.style.opacity = op;
mc = root.FindChildTraverse("StatsAndModsContainer"); mc.style.x = ox+"px"; mc.style.y = (-oy)+"px";
bp = root.FindChildTraverse("hud_signature"); bp.style.x / y / uiScale / washColor / opacity
ap = root.FindChildTraverse("ammo_panel"); ap.style.x = ox+"px"; ap.style.y = (80-oy)+"px";
```
Property inventory seen: `x`, `y` (CSS `transform` equivalents via `style.x/y`), `ui-scale`, `opacity`, `wash-color`, `margin-*`, `horizontal-align`, `vertical-align`, `visibility: collapse|visible`, `flow-children`, `width/height`, `transform: translateX() translateY() rotateZ()`, `pre-transform-scale2d`, `overflow: noclip`, `z-index`. [V]

**Blur Disabler (`pak97_dir.vpk`)**: stub `panorama/styles/citadel_base_styles.vcss_c` (1324 bytes compiled) = `@import ".../base/citadel_base_styles.vcss_c"; @define ingameHudBlur: none; @define menuBlur: none;`, plus a 197,199 byte `base/` copy. Confirms `@define` overrides after `@import` as a viable theming hook (vanilla minimap frame uses `world-blur: ingameHudBlur`). [V on contents; that the define override actually takes effect in game: I (it is the mod's whole purpose)]

### 3.5 DeadTune-oriented summary of what a CSS-only editor can do

Without touching layouts: move/resize/scale/hide (visibility:collapse) for `#minimap_persp` (+ container/frame), `#health_and_abilities_container`, `#hud_signature`, `#ActiveAbilitiesMenu`, `#hud_passive_items`, `#StatsAndModsContainer`/`#LowerLeft`/`#hudPlayerStats`, `CitadelHudTopBar`, via `margin-*`, `horizontal-align`/`vertical-align`, `width/height`, `ui-scale`, `transform`, `opacity`, `visibility`. Rules go into the matching file(s) from the table (mostly `hud.vcss_c`; top bar in `citadel_hud_top_bar.vcss_c`). [I]
Needs new panels or JS: any new widget, dynamic behaviour, user-adjustable at runtime, since that requires hud.vxml_c edits. [I]

## 4. VPK writing requirements

Facts from all 9 VPKs plus a reader/writer I wrote (`$S/vpkr.py`, `$S/gen/mkvpk.py`). [V unless marked]

- **Single `pakNN_dir.vpk` file is enough.** Every mod VPK is VPK v2 (signature 0x55AA1234, version 2, 28-byte header), every entry has `archive_index = 0x7fff` ("data follows the tree in this same file"), so no `_000.vpk` is needed. Header: `tree_size, file_data_section_size, archive_md5_section_size=0, other_md5_section_size=48, signature_section_size=0`. [V]
- **Entry record**: `crc32 (u32), preload_bytes (u16)=0, archive_index (u16)=0x7fff, entry_offset (u32, relative to start of file data section = 28+tree_size), entry_length (u32), terminator 0xFFFF`. Preload bytes were 0 in all entries. [V]
- **CRC32 per entry** = `zlib.crc32` of the whole file bytes; all 2,589 entries in the 9 files verify (0 mismatches). [V]
- **Tree layout**: `ext\0 dir\0 name\0 <record> ... \0 (end names) \0 (end dirs) \0 (end ext)`; root directory is stored as a single space `" "`; extension without dot (`vcss_c`), name without extension, forward slashes. [V]
- **Ordering and alignment are not required**: the mod trees are not sorted (ext order e.g. vdata_c, vcss_c, vjs_c, vtex_c...), and data offsets are unaligned. Data is laid out contiguously in tree order. [V on mods; that the game tolerates arbitrary order is [I] because these mods are in public use; I could not run the game]
- **MD5 section (48 bytes at the end)**: qollite ships it all zeros (VRF `--vpk_verify` complains "File tree checksum mismatch ... expected 00..."), the other 8 ship non-zero. So the game does not require valid MD5s [I]. My writer produces `md5(tree) | md5(empty archive section) | md5(whole file up to here + first 32 bytes of section)`, and VRF verifies it ("Success", also on qollock's pak03, so the scheme matches the others). Zero-filled is also acceptable evidence-wise. [V for VRF; I for the game]
- **Install location and mounting**: `game/citadel/addons/pakNN_dir.vpk`; mounted only if `gameinfo.gi` SearchPaths has `Game citadel/addons` (from `research/reference/Deadlock-Config/README.md:50` and `deadlock-mod-manager-excerpts/game_config_manager.rs`). OptimizationLock presets already ship that line. [V from repo text; U in game]
- **Priority between addons**: file names pak01...pak99 are used by mods in the sample (01, 02, 03, 26, 89, 97). Whether lower or higher number wins, and how multiple addons that replace the same path interact, is [U]. Qollite and qollock both replace `hud.vcss_c`, so they cannot be combined; the editor must own one HUD VPK and warn about conflicts.
- Valve updates change `pak01_dir.vpk` of the game itself; addon files override per path, so a stale override of a changed file breaks the HUD in subtle ways. Detect `buildid` changes (already in PLAN.md) and regenerate. [I]
- Size: our generated minimal pak with one 119 KB file is 119,173 bytes. [V]

## 5. Tooling notes

- Source2Viewer-CLI 20.0 macOS arm64 downloaded to `$S/tools/Source2Viewer-CLI` (`cli-macos-arm64.zip` from github.com/ValveResourceFormat/ValveResourceFormat/releases/tag/20.0). Runs without Gatekeeper trouble in this environment. Useful flags: `-i file.vpk -o outdir -d -f panorama/ -e vcss_c,vxml_c,vjs_c`, `-b RED2|DATA|SrMa|LaCo`, `--vpk_list`, `--vpk_verify`. [V]
- VRF decompiles `.vcss_c` to prettified `.css`, `.vxml_c` to `.xml` (include `src` lines lose/keep `_c` as authored), `.vjs_c` to `.js`. These are for reading, not round-tripping: the prettifier reformats.
- Scripts: `$S/vpkr.py` (VPK reader), `$S/ana.py` (resource header/block parser), `$S/extract.py` (pull one file out of a VPK), `$S/gen/mkvcss.py` (template clone + DATA replace), `$S/gen/mkvpk.py` (VPK v2 writer). In Rust, these are ~150 lines total plus a crc32 crate.
- No `vpk` Python package was needed.

## 6. Open questions and recommended next experiments (all need the real game)

1. Load `$S/gen/pak99_dir.vpk` (hud.vcss_c with one appended rule) in the game under `citadel/addons/` and see if the minimap resizes. This answers engine acceptance of generated vcss_c (2.5) and ordering/MD5 tolerance (4) in one shot.
2. Variants if it fails: (a) prefix = crc32(text) only, (b) drop SrMa with prefix = crc32(text), (c) zero MD5 section.
3. Build a `.vjs_c` generator only if runtime-adjustable HUD (needs hud.vxml_c LaCo) becomes a goal; otherwise stay CSS-only.
4. Determine addon priority by shipping two addons overriding the same file.
5. Confirm top bar and ability-icon internals (class names inside `hud_ability_icon.vcss_c`, `citadel_hud_top_bar_player.vcss_c`) per game build by decompiling the user's own `pak01_dir.vpk` at runtime with the same code path as the generator.
