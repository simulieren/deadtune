# Top bar mods: what they change, what DeadTune rebuilds

Research notes for the Top bar page and for the generic layout/script injection in `dt-core`.
Date: 2026-10-05. Scripts that produced every fact below: `scripts/` next to this file
(`fetch.py` pulls Valve's decompiled sources from SteamTracking/GameTracking-Deadlock,
`res.py` dumps a mod VPK, `cssdiff.py` diffs two stylesheets at declaration level, `kv3.py`
decodes a compiled layout, `kv3inv.py` inventories its node types).

Licence note: all three top bar mods and the minimap mod are CC BY-NC-ND 4.0. Nothing from
their files is shipped or committed. DeadTune rebuilds the ideas from the game's own files and
credits the authors. Their VPKs stayed in the session scratchpad.

Markers: **[V]** verified by running code on the files; **[I]** inferred; **[U]** needs the game.

## 1. The three top bar mods

| Mod | Author | Files in the VPK | Built with |
|---|---|---|---|
| Top bar HUD hero visibility improvement (GameBanana 619963) | NA-45 | `panorama/styles/citadel_hud_top_bar.vcss_c` (full replacement, 52 870 bytes of text) | Valve's compiler, 2026-06-05 |
| Minimal Topbar (651169), a fork of Top Bar Plus | Stovven | `layout/citadel_hud_top_bar.vxml_c`, `layout/citadel_hud_top_bar_player.vxml_c`, `scripts/rejuvnbufftimer.vjs_c`, `scripts/urntracker.vjs_c`, `styles/citadel_hud_top_bar.vcss_c`, `styles/hero_testing_menu.vcss_c`, `styles/hud_damage_report.vcss_c` | Valve's compiler (KV3 v5 layouts) |
| [UPDATED] Top Bar Plus (623518), 2026-10-01 | bonclide | `layout/citadel_hud_top_bar.vxml_c`, `layout/citadel_hud_top_bar_player.vxml_c`, `layout/citadel_hud_hero_shop.vxml_c`, `layout/hud_paused.vxml_c`, `scripts/topbar_rank_hud.vjs_c`, `scripts/recent_purchases_redux.vjs_c` (+ `_data.vjs_c`, 374 KB), `styles/citadel_hud_top_bar.vcss_c` (stub) + `styles/base/{citadel_hud_top_bar,hud,hud_paused}.vcss_c` (vanilla copies), `styles/hud.vcss_c`, `styles/hud_paused.vcss_c` (stubs) | Valve's compiler (KV3 v4 layouts, an older toolchain) |

### 1.1 NA-45: declaration-level diff against vanilla

Vanilla reference: GameTracking commit `24ebfd591c` (build 6552, 2026-06-04, the last change
to `citadel_hud_top_bar.css` before the mod's date) and `26418884bd` (6555, 2026-06-06).
The mod is a whole-file replacement; against build 6552 it differs in exactly three
declarations, everything else is byte-for-byte the same minified text. [V]

| Change | Selector | Declaration |
|---|---|---|
| Added rule (the mod's whole point) | `CitadelHudTopBarPlayer:not(.HealthVisible) #HeroContents` | `opacity: 0.2` |
| Changed | `.TeamNetworth` | `width: 300px` -> `200px` |
| Changed | `.TeamScore` | `width: 145px` -> `95px` |

The two width changes narrow the team soul display ("post-urn-fix-v3" in the file name).
Against today's vanilla (`master`, build 6728) the same three differences hold; vanilla itself
has since dropped the `#IdolCashInMeter` (soul urn meter) rules and gained `#KothCashInMeter`,
`.showNewTopbar` and the `#GradientBacker` rules, which a stale full copy would lose. That is
why DeadTune appends to the installed game's own stylesheet instead of shipping a copy.

`.HealthVisible` is the game's own state class on `CitadelHudTopBarPlayer`: vanilla uses it only
for `.HealthVisible #HealthBar { opacity: 1 }`, so by default an enemy out of vision just loses
the health bar. NA-45 dims the whole portrait (`#HeroContents`) when the class is absent. [V]

### 1.2 Stovven, Minimal Topbar

Layouts (decoded from the KV3 `LaCo` block, diffed against the June vanilla so the mod's own
edits are separated from stale vanilla): [V]

- `citadel_hud_top_bar.xml`: adds `<scripts>` with `rejuvnbufftimer.vjs_c` and
  `urntracker.vjs_c`; adds `#UrnTracker` (with `#UrnTrackerLabel`, `#UrnTrackerPercentageLabel`)
  inside `.TeamNetworth`; adds `#BuffHUD`, `#BuffSplitHUD`, `#RejuvHUD`, `#RejuvSplitHUD`,
  `#RejuvBuff`, `#Buff`, `#Rejuv` panels (labels plus `Image` with
  `s2r://panorama/images/hud/modifiers/icon_powerup.svg` / `icon_rejuvenator.svg`) after
  `#ObjectivesMap`; removes the vanilla `#MidbossTimerLabel`. Its vanilla base predates the
  Koth meter (it still carries `#IdolCashInMeter`).
- `citadel_hud_top_bar_player.xml`: kill streak label bound to `{d:kill_streak}`, removes the
  `.HeroName` label, drops `always-cache-composition-layer`. Cosmetic.

Stylesheet (`cssdiff.py` against today's vanilla, ignoring the stale-copy noise): [V]

- Missing enemies: `CitadelHudTopBarPlayer:not(.HealthVisible) #HeroImageArea { wash-color: #00000090 }`
  (darkens the portrait instead of fading it).
- Smaller soul lead: `.TeamScore { width: 145px -> 125px }`, `.GameClock { width: fit-children -> 200px }`,
  `.GameTime { font-family: numericBlock -> block }`.
- Timer chips beside the clock: `#BuffHUD`/`#RejuvHUD` 75x30 px, `ignore-parent-flow`,
  `x: +-60px; y: 9px`, labels `font-size: 14px; font-family: block`, warning colours
  `.yellow { color: #cfb430 }`, `.red { color: #cf3030 }`, hidden under `.connectedToHideout`
  and `.gamemode_streetbrawl`.
- Urn tracker: `#UrnTracker` 35 px wide, `wash-color` by state `.good #67ab8f`, `.bad #ff5757`,
  `.neutral offWhite`, label `font-size: 23px; font-family: oracle`.
- Assorted: respawn timer moved up, death icon shrunk to 13x15, KDA labels faded, lane swap
  button enlarged and faded, `.RespawnTimer` and `.SoulsValue` fonts changed.
- `hero_testing_menu.vcss_c` and `hud_damage_report.vcss_c`: sandbox menu and damage report
  restyles (collapses `#hero_tools,#game_rules,#hero_control` until expanded, removes the pin and
  free-cursor buttons). Not top bar; out of scope here.

Scripts (plain JavaScript in the `DATA` block): [V]

- `rejuvnbufftimer.js`: reads the game clock through `Game.GetDOTATime()`, `Game.GetGameTime()`,
  `Game.Time`, `Game.GameTime`, `GameUI.GetGameTime()` in that order and, when none exists,
  parses the `#GameTime` label text (`mm:ss`). Powerup ("bridge buff") countdown is
  `300 - (t mod 300)` seconds. Rejuvenator countdown runs the fixed phase table
  `[1, 413, 353, 293]` seconds (first spawn, then three shorter respawn phases), advances a phase
  when a `RejuvCount_1..4` class appears on `#RejuvenatorFriendly` / `#RejuvenatorEnemy`
  (the game's own team charge icons), and shows a 180 s "rejuv buff" countdown from that moment.
  Hides everything while `#Hud` has `connectedToHideout`. Polls with `$.Schedule`.
- `urntracker.js`: reads the two `.ScoreLabel` texts under `#TeamScoreFriendly` /
  `#TeamScoreEnemy` (the game's team net worth, `{s:team_networth}`), parses `12.3k` style
  numbers, shows `|friendly - enemy| / enemy` as a percentage with `good`/`bad`/`neutral`
  classes at a 15 % threshold before minute 15 and 10 % after.

### 1.3 bonclide, Top Bar Plus

Layouts: [V]

- `citadel_hud_top_bar.xml`: `<scripts>` with `topbar_rank_hud.vjs_c`; `onload` attribute on
  `CitadelHudTopBar` calling `$.TopbarRankHudRootLoaded`; adds `#TopbarRankAdvantage` (label
  `#TopbarRankAdvantageLabel`) inside `.TeamNetworth`; adds `#RejuvenationTimer` label inside
  `#RejuvenatorTimer` and a `#BridgeTimer` panel (same structure as the vanilla rejuv timer:
  `StatusEffectsBorder`, `StatusEffectCooldownOverlay`, `#BridgeBackground`, `StatusEffectImage`,
  `.statusEffectImage`, label `#BridgeTimerLabel`) inside `#RejuvenatorCharges`.
- `citadel_hud_top_bar_player.xml`: adds `<Label class="HeroNameHidden" text="{g:citadel_hero_name:hero_id}" />`
  (collapsed by CSS; the purchases script sets the `hero_id` dialog variable from
  `#HeroBadge.heroid` and reads the localised hero name back).
- `citadel_hud_hero_shop.xml`: `<scripts>` with the two purchase scripts; adds
  `<Label class="recentModPurchaserHero" text="{s:recent_hero_name}" />` to the game's
  `RecentPurchase` snippet; reorders `#MainPanel` after `#NavPanel`; replaces the
  `EnterPurchaseKey` binding with a label.
- `hud_paused.xml`: the pause overlay is reduced to a "II" label.

Stylesheet (`@import` of a vanilla copy under `base/`, then the mod's rules): [V]

- `CitadelHudTopBarPlayer:not(.HealthVisible) #HeroContents { opacity: 0.2 }` (NA-45's rule).
- Bridge timer styled like the vanilla midboss timer (`.bridgTimer` 30x30, label
  `font-size: 10px; font-family: block; opacity: 0.3`), warning washes
  `.TopbarRankWarningYellow #FFFF0030`, `.TopbarRankWarningRed #FF000030`.
- `.TopbarRankAdvantage` 145 px, `wash-color` `#00FF7F` good, `#CD5C5C` bad, `#DCDCDC` neutral.
- `#RejuvenatorCharges` always visible and 25 px lower; `#BackgroundStrip` and
  `#PlayerNameNWContainer` lose their blur; `.objDamageContainer` shown.
- Quick purchases popups: `.QuickPurchasesPanel` (`ignore-parent-flow`, flows up, `z-index: -99`),
  `.quickPurchase` entries at `ui-scale: 75%`, borders and backers coloured by the game's own
  purchase classes (`isTier1..4Purchase`, `isWeaponPurchase`, `isArmorPurchase`,
  `isTechPurchase`), 0.3 s fade in/out, a glow animation for tier 4.
- `hud.vcss_c` stub: `.HudTakeoverEnabled .HudCore { opacity: 1 }`. `hud_paused.vcss_c` stub:
  shrinks the pause panel, hides it while the shop is open. Not top bar; out of scope here.

Scripts: [V]

- `topbar_rank_hud.js` (despite the name, it reads no rank anywhere): same clock probing as
  Stovven's script; powerup countdown `300 - (t mod 300)`; rejuvenator phases `[413, 353, 293]`
  advanced by the `RejuvCount_N` classes; 180 s rejuv buff; soul advantage from the two
  `.ScoreLabel` texts as `+x.x%` with the same 15 %/10 % thresholds; per player an "unspent
  souls" row computed as `SoulsValue` minus the tier costs (800/1600/3200/6400) of the items in
  `#PlayerModsContainer`. Hidden in the hideout and in street brawl.
- `recent_purchases_redux.js`: polls the shop's `#RecentPurchasesContainer` (the game fills it
  with `.recentPurchase` panels carrying `{s:recent_mod_name}`, `{s:time_purchased}`,
  `{s:recent_tier_purchased}` and team/tier/category classes), adds filter toggle buttons to the
  shop's `#RecentPurchasesPanel`, caps the list at 50, and for every new purchase creates a
  popup under the buying hero's portrait (`$.CreatePanel` into the `CitadelHudTopBarPlayer`
  whose hidden hero-name label matches the purchase's hero name). Icons come from the
  `_data.js` table mapping every localised item name to its `s2r://panorama/images/items/...`
  texture.

Player rank: the mod's current files contain no rank display and read no rank data. The only
"rank" is the script's file name. Panorama exposes no rank API for other players and the stock
in-match HUD shows none, so there is nothing to rebuild. DeadTune ships no rank feature; this
note replaces it.

### 1.4 What each DeadTune feature reads (for the reader, per Simon's decision to keep them all)

| Feature | Idea by | Source of the data in the HUD | DeadTune's version |
|---|---|---|---|
| Missing enemies dimmed | NA-45 (also in both other mods) | The game's `.HealthVisible` class on each `CitadelHudTopBarPlayer` | CSS only: `#TeamEnemy CitadelHudTopBarPlayer:not(.HealthVisible) #HeroContents` gets the chosen opacity, saturation and darkening |
| Spawn timers | Stovven, bonclide | The game clock (`#GameTime`, `{s:game_clock}`), the game's `#RejuvenatorTimer` label `{s:midboss_timer}` with its `.midboss_cooldown` class, and the `RejuvCount_N` classes on the team charge icons | Our script shows `300 - (t mod 300)` for the powerups; for the rejuvenator it shows the game's own midboss timer text when the game shows one, else the first-spawn countdown from the phase table |
| Urn soul lead | bonclide (Stovven's fork) | The two team net worth labels (`{s:team_networth}`) already on the bar | Our script shows the percentage gap with the mods' 15 %/10 % thresholds |
| Smaller soul lead | Stovven | None (presentation) | `.TeamNetworth` scaled down |
| Recent purchases on portraits | bonclide | The shop's `#RecentPurchasesContainer` (`.recentPurchase` panels with name, time, tier and team classes) and each portrait's `#HeroBadge.heroid` | Our script copies new purchases to a popup under the buyer's portrait, item name only, coloured by the game's tier/category classes; no icon table |

Everything above re-presents data the stock HUD already draws or derives it from the public
game clock. Nothing is read from entities, networked tables or memory; Panorama scripts can't.

## 2. Compiled resource facts that matter for injection

From the mods' files and ValveResourceFormat's `Panorama.cs`, `PanoramaLayout.cs`,
`BinaryKV3.cs` (fetched to the scratchpad). [V]

| Type | Header type version | Blocks Valve ships | DATA layout | Plaintext alternative seen in the wild |
|---|---|---|---|---|
| `.vcss_c` | 3 | RED2, DATA, SrMa | `u32 prefix, u16 images, image table, CSS text`; prefix = source CRC xor crc32(text) | DATA only, prefix = crc32(text), no images (FesamAyt's minimap mod) |
| `.vxml_c` | 3 | RED2, DATA (6 bytes), LaCo | DATA is `u32 crc32(source xml), u16 0`; LaCo is binary KV3 (v4 or v5, LZ4, format GUID `7c161274-e906-9846-aff2-e63eb59037e7`) holding `m_AST.m_pRoot` | DATA only: `u32 crc32(text), u16 image count, (name\0, u16 w, u16 h, u32 crc) per image, XML text`; no LaCo (FesamAyt's `hud_minimap.vxml_c`, 6653 bytes) |
| `.vjs_c` | 4 | RED2, DATA | DATA is the JavaScript text, no prefix | DATA only (FesamAyt, Stovven and bonclide all ship scripts this way or with a foreign RED2) |

KV3 layout AST (all six mod layouts, `kv3inv.py`): keys `m_AST`, `m_pRoot`, `eType`, `name`,
`vecChildren`, `child`, `sourceLineColumn`; node types `ROOT`, `STYLES`, `SCRIPTS`, `SNIPPETS`,
`SNIPPET`, `INCLUDE`, `REFERENCE_COMPILED`, `PANEL`, `PANEL_ATTRIBUTE`, `PANEL_ATTRIBUTE_VALUE`
(VRF also knows `SCRIPT_BODY` and `REFERENCE_PASSTHROUGH`); KV3 value types used: STRING,
OBJECT, INT32, plain and typed arrays; no flags, no binary blobs. [V]

The plaintext `.vxml_c` is the key finding: a published, working mod replaces
`panorama/layout/hud_minimap.vxml_c` with the decompiled vanilla XML plus one `<include>` in
`<styles>` and a `<scripts>` block, stored as text. So DeadTune needs a KV3 reader (to decompile
the installed game's current layout) and an XML printer, and no KV3 writer. The image table in
that file lists the five `.vtex` referenced by `src=` attributes with their pixel sizes and a
CRC that is neither the path's crc32 nor the resource name's; it is probably the VPK entry CRC of
the texture file [U]. The top bar layout references no textures, so its table is empty.

## 3. Generic API in dt-core (for the minimap follow-up)

Module `hud::inject` (name final once merged; check `hud/mod.rs`). Everything builds from the
installed game's own `pak01_dir.vpk` at apply time, so a game update rebuilds it.

```rust
/// Our own files live under these prefixes; nothing of the game's is ever at these paths.
pub const SCRIPTS_DIR: &str = "panorama/scripts/deadtune/";
pub const STYLES_DIR: &str = "panorama/styles/deadtune/";

/// Where to put something in a layout, by the `id` attribute of an existing panel.
pub enum Anchor { AppendTo(String), Before(String), After(String) }

/// One layout's edit. Includes are `s2r://` paths of files in `HudPatch::own_files`
/// (or the game's). Panels are XML snippets; most features create panels from script instead.
#[derive(Default)]
pub struct LayoutEdit {
    pub style_includes: Vec<String>,
    pub script_includes: Vec<String>,
    pub panels: Vec<(Anchor, String)>,
}

pub struct HudPatch {
    /// Minified CSS appended to the game's compiled stylesheet at this path (existing path).
    pub styles: BTreeMap<String, String>,
    /// The game's layouts to rebuild as text with these additions.
    pub layouts: BTreeMap<String, LayoutEdit>,
    /// Our own plaintext files: `.vjs_c` under SCRIPTS_DIR, `.vcss_c` under STYLES_DIR.
    pub own_files: BTreeMap<String, String>,
}

/// The game's compiled layout -> its XML text (KV3 LaCo decoded, or the text it already holds).
pub fn layout_text(compiled: &[u8]) -> Result<String, InjectError>;
/// `layout_text` plus the edit, as a plaintext `.vxml_c` resource.
pub fn patched_layout(compiled: &[u8], edit: &LayoutEdit, images: &[ImageEntry]) -> Result<Vec<u8>, InjectError>;
/// Plaintext `.vjs_c` (type 4) and `.vcss_c` (type 3, prefix crc32(text)).
pub fn script_resource(js: &str) -> Vec<u8>;
pub fn style_resource(css: &str) -> Vec<u8>;
```

`install::build_addon` turns a `HudPatch` into the VPK: each `styles` entry is
`resource::append_style(game file, css)`; each `layouts` entry is `patched_layout(game file, edit)`;
each `own_files` entry is `script_resource` or `style_resource` by extension.
`addons::verify` checks a rebuilt layout by regenerating it from the game's file (the text must
contain every line of the decompiled original in order) and own files by their prefix CRC.

Minimap follow-up, concretely: `layouts["panorama/layout/hud_minimap.vxml_c"]` with
`style_includes = ["s2r://panorama/styles/deadtune/minimap_x.vcss_c"]`,
`script_includes = ["s2r://panorama/scripts/deadtune/minimap_x.vjs_c"]`, and the two files in
`own_files`. The minimap layout references five `.vtex` images; pass their table (sizes from
`texture::vtex::Vtex`, CRC per section 2) or, first, try an empty table and let the Windows
checklist decide [U].

## 4. What to prove on Windows

- The game loads a plaintext `citadel_hud_top_bar.vxml_c` rebuilt by DeadTune (top bar visible,
  no `FATAL` in `console.log`); the launch guard removes the pak if the start fails.
- With "Missing enemies dimmed" on, an enemy portrait fades when the enemy leaves vision and
  returns when seen again; allies and the local player never dim.
- Spawn timers count down and the rejuvenator chip shows the game's own timer text after the
  first kill. Urn lead shows a percentage and changes colour at the thresholds. Purchases pop
  under the right portrait and fade after 10 s.
- `deadtune-cli addons verify` reports `ok` for `pak77_dir.vpk`.
