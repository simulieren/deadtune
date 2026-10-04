# Minimap icon colours

Question: can DeadTune recolour minimap icons (heroes, allies vs enemies, troopers, objectives, ziplines), and how?

Status: research only, 2026-10-04. Nothing tested in game.

Tags: **[V]** verified this session by reading the file or listing named next to it. **[I]** inferred from what was read. **[T]** needs an in-game test.

Sources:
- Vanilla CSS and layout: `$S/dec/van/styles/hud_minimap.css`, `$S/dec/van/layout/hud_minimap.xml` (`$S` = the deadlock-config-editor scratchpad). The copy is current: it diffs identical (apart from the decompiler banner) to `game/citadel/pak01_dir/panorama/styles/hud_minimap.css` and `layout/hud_minimap.xml` in SteamTracking/GameTracking-Deadlock at its latest commit, 2026-10-03. [V]
- Game file listing: `GameTracking-Deadlock/game/citadel/pak01_dir.txt` (fetched to this session's scratchpad). [V]
- ConVars: `research/configs/OptimizationLock/convars.txt` (has min/max and descriptions), `cvarlist.txt`, `catalog/hud.toml`.
- Mods: QoL Lite and QoL Lock decompiled under `$S/dec/qollite`, `$S/dec/qollock`.

## 1. Short answer

| Icon | How it is drawn | Recolour possible? | Lever |
|---|---|---|---|
| Hero icons (allies, enemies, self) | Panorama panels from `MinimapPlayerSnippet`, classes `map_button player friend/enemy/team1/team2/localplayer` | Yes | CSS on `#BackgroundImage`, `#ArrowImage` |
| Objectives (guardians, walkers, patron, barracks) | Panels from `MinimapBossSnippet`/`MinimapTier3BossSnippet`, class `boss` + `friend/enemy` + lane class | Yes | CSS `wash-color` on `.boss_image` |
| Mid boss, neutral camps, powerups, soul urn, shops, teleporters | Panels with a `background-image` | Yes | CSS `wash-color`, `saturation`, `brightness`, `hue-rotation` |
| Troopers | Drawn into the `<UICanvas id="canvas">` by game code | Not per team with CSS | Texture swap (`npc_marker_*_psd.vtex_c`), or a tint of the whole canvas |
| Ziplines | Same canvas | Not per team with CSS | Texture swap (`zip_line_marker_*_psd.vtex_c`) if they are textured; else no lever |
| Enemy colour, game-wide | Official setting | Yes, maybe including the minimap | ConVars `citadel_custom_ui_colors` + `citadel_enemy_ui_color_r/g/b` [T] |

The recommended lever is CSS appended to `panorama/styles/hud_minimap.vcss_c`, the file that owns these rules. DeadTune's existing pipeline already supports it (section 4). Troopers and ziplines need texture work that DeadTune cannot do today.

## 2. ConVar lever

### 2.1 What exists

| ConVar | Default | Flags | Apply class | Note |
|---|---|---|---|---|
| `citadel_custom_ui_colors` | false | clientdll archive per_user | live | "Enable colorblind-friendly colors and the player's custom UI colors." [V convars.txt:2126] |
| `citadel_enemy_ui_color_r` | 215 (0-255) | clientdll archive per_user | live | "color used for enemy health bars and other enemy indicators in the UI" [V convars.txt:2528] |
| `citadel_enemy_ui_color_g` | 50 | same | live | [V :2525] |
| `citadel_enemy_ui_color_b` | 50 | same | live | [V :2522] |
| `citadel_use_spectator_team_colors` | false | developmentonly clientdll defensive | restart | "Forces HUD and game to draw team colors as if you are a spectator" (amber/sapphire instead of friend/enemy) [V :5513] |
| `citadel_zipline_show_lane_colors_for_spectators` | false | developmentonly clientdll | restart | Spectator only by name [V :5702] |
| `r_colorblindsim_*` | 0 / mode 1 | clientdll release | live | By name these simulate colour blindness on the whole frame, which is a developer check, not a fix. Not a recolour lever. [I] |

The apply class follows the `hud::convars` rule (devonly → restart, cheat → live_cheat, else live). [I, not run]

### 2.2 What does not exist

No ConVar for an ally colour, a trooper colour, a zipline colour, an objective colour or a per-icon colour. Searched `cvarlist.txt` and `convars.txt` for minimap, color, colour, colorblind, wash, team, friend, enemy, ally, lane. [V by grep]

The four enemy-colour ConVars are not in `catalog/hud.toml`. [V by grep]

`research/hud/web.md` lists the enemy colour default as 255/229/91. `convars.txt` says 215/50/50 with an explicit 0-255 range. Trust `convars.txt`; fix `web.md` when next touched. [V both files]

### 2.3 Use

- These are the official "Enemy UI Color" accessibility setting from the 2026-09-29 update (see `web.md` 3.3). No vanilla CSS, layout or listed file mentions the setting, so game code applies the colour, not a stylesheet. [V grep of all vanilla CSS and the pak01 listing; I for the conclusion]
- Whether it reaches minimap enemy icons is unknown. The description says "health bars and other enemy indicators". [T]
- If game code applies it as an inline style on the panel, inline style beats any stylesheet rule, so our CSS for enemy icons would stop working while `citadel_custom_ui_colors 1`. [I, T]
- `per_user` means the value lives in the per-account config, not `machine_convars`. Check where the apply pipeline writes archive per_user ConVars before adding them. [I]
- Do not use `citadel_use_spectator_team_colors`: developmentonly, so gameinfo.gi only, with the matchmaking risk that comes with gameinfo ConVar edits, and it changes the whole HUD. [I]

## 3. CSS lever

### 3.1 Where the rules live

- `hud.xml` places `<HudMinimap id="hud_minimap">` inside `#HudMinimapContainer`. [V `$S/dec/van/layout/hud.xml:215`]
- `hud_minimap.xml` includes `citadel_base_styles.vcss` and `hud_minimap.vcss`, and holds the snippets the game instantiates per icon. [V]
- So all minimap icon rules are in `panorama/styles/hud_minimap.vcss_c` (75,522 bytes in the current pak01), not in `hud.vcss_c`. [V pak01 listing]
- QoL Lite overrides `hud_minimap.vcss_c` with rules like `#hud_minimap.bm_size_ally_50 .map_button.player.friend`, and QoL Lock overrides it with `.qol_minimap_elevation_markers_active .map_button.player.arrowUp #ArrowImage { wash-color: orange; }`. That shows the `friend`/`enemy` classes and `#hud_minimap` ancestor are usable from that file, and that recolouring `#ArrowImage` with `wash-color` is something a shipped mod already does. [V for what the mods ship; I that it works in game]

### 3.2 Colour defines

| Define | Value | File |
|---|---|---|
| `colorEnemy` | `#FF410D` | `citadel_base_styles.css:15` |
| `offWhite` | `#FFEFD7` | `citadel_base_styles.css:23` |
| `team1Color` (amber) | `#D4860B` | `citadel_base_styles.css:64` |
| `team2Color` (sapphire) | `#4D75C3` | `citadel_base_styles.css:66` |
| `ColorEnemyObjective` | `#DC4C2FFF` | `hud_minimap.css:15` |
| `ColorTeam1` / `ColorTeam2` | `#E7B659` / `#5B79E6` | `hud_minimap.css:9-14` |

[V] `@define` is resolved when the sheet compiles, so redefining a name in appended text does not change earlier rules. Override the rules, not the defines. [I]

### 3.3 Hero icons

Snippet children: `#HoverPanel`, `#HoverPanelAccurate`, `#MainImage` (hero portrait), `#SpeakingImage`, `#DeathImage`, `#HeldIdolImage`, `#BackgroundImage`, `#ArrowImage`, `#FrogImage`, `#LocalSpecularImage`. [V hud_minimap.xml]

| Vanilla rule | Line | What it colours |
|---|---|---|
| `.friend.player #BackgroundImage { background-color: offWhite; }` | 1522 | Ally disc behind the portrait (local player too, it has `friend`) |
| `.enemy.player #BackgroundImage { background-color: colorEnemy; }` | 1527 | Enemy disc |
| `.player.team1 / .team2 #BackgroundImage { background-color: team1Color / team2Color; }` | 1512, 1517 | Spectator view |
| `.player #BackgroundImage { … opacity: 0.9; border-radius: 50px; background-size: 80% 80%; }` | 1500 | Shape |
| `.friend.player / .enemy.player #ArrowImage { background-image: player_arrow_friend/enemy_psd.vtex; }` | 1558, 1563 | Edge arrow, shown with `.playerWasClamped` |
| `.player.localplayer .button_image { wash-color: white; }` | 1469 | Self marker |
| `.PlayerDead #BackgroundImage { opacity: 0.6; }` | 1337 | Dead state |

[V] The team colour of a hero icon is the `background-color` of `#BackgroundImage`. That is a flat fill, so `background-color` gives an exact colour; `wash-color` would only tint it. [I]

### 3.4 Objectives

Guardians, walkers, barracks and the patron use the boss snippets: `.boss_health_bg boss_image` and `.boss_health boss_image` panels with textures. [V]

| Vanilla rule | Line |
|---|---|
| `.map_button.enemy.boss .boss_image { wash-color: ColorEnemyObjective; }` | 683 |
| `.map_button.friend.boss .boss_image { wash-color: offWhite; }` | 678 |
| `.map_button.friend.boss.yellowLane .boss_image { wash-color: #FFDF40; }` | 688 |
| `… .greenLane … { wash-color: magenta; }` | 693 |
| `… .blueLane … { wash-color: #2EC7E6; }` | 698 |
| `… .purpleLane … { wash-color: #6BB247; }` | 703 |
| `.map_button.boss.team1 / .team2 .boss_image { wash-color: ColorTeam1 / ColorTeam2; }` | 668, 673 |

[V] Allied objectives are coloured by lane, enemy objectives by one colour. The lane rules have one more class than the plain friend rule, so a recolour of allied objectives must target each lane selector, or use a higher-specificity selector.

### 3.5 Other panel icons

All are `.map_button.<type>` with a `background-image` and often a `wash-color`. [V]

| Selector | Vanilla colour | Line |
|---|---|---|
| `.map_button.mid_boss .boss_image` | `saturation:0; brightness:0.3; opacity:0.5` | 571 |
| `.map_button.neutral_weak/_medium/_large/_vault` | none (texture colour) | 539-557 |
| `.map_button.powerup_gun/_souls/_survival/_casting/_movement` | `#FFCC80` / `shardColor` / `#D7FF9A` / `#E1A6FF` / `#99FFDD` | 401-434 |
| `.map_button.gold_spawn`, `.rejuv_crystal` | `rgb(130,255,249)` | 368-382 |
| `.map_button.idol_spawn` (soul urn) | `img-shadow … shardColor&50` | 360 |
| `.map_button.idol_return_enemy .idolReturnTarget` | `ColorEnemyObjective` | 890 |
| `.map_button.tier1_shop`, `.corrupted_item_shop` | texture, `img-shadow` black | 987, 1094 |
| `.map_button.teleporter_*` | texture | 459-502 |
| `.client_cone_fov` | `rgb(178,255,229)` | 289 |

### 3.6 Troopers and ziplines

- No snippet, class or rule for troopers or ziplines exists in `hud_minimap.css` or `hud_minimap.xml`. [V]
- Their only traces: the ConVars `citadel_minimap_trooper_size`, `citadel_minimap_koth_trooper_size`, `citadel_minimap_zip_line_thickness` (numbers read by game code), the `.precache` rule that loads `npc_marker_enemy/friend/neutral_psd.vtex` and `zip_line_marker_enemy/friend/neutral_psd.vtex`, and `.HideOldZiplines #canvas { visibility: collapse; }`. [V hud_minimap.css:210, 1602-1635]
- Conclusion: game code draws troopers and (old) ziplines into `<UICanvas id="canvas">` using those textures. CSS cannot reach individual canvas draws. [I]
- CSS can tint the whole canvas (`#hud_minimap #canvas { wash-color: … }`). That changes every trooper and zipline of both teams together, which defeats the purpose. [I]
- The textures exist in the game pak at `panorama/images/minimap/npc_marker_{enemy,friend,neutral}_psd.vtex_c` and `zip_line_marker_{enemy,friend,neutral}_psd.vtex_c`, plus `npc_tower_*`, `npc_dispenser_friend`, `player_marker*`, `player_arrow_*`. [V pak01 listing]
- An addon can replace a `.vtex_c` per path. QoL Lite ships `materials/minimap/neutral_vault.vtex_c` and three `panorama/images/minimap/base/*_custom_png.vtex_c`. [V listing.txt; I that the swap takes effect]
- Whether the canvas really uses the `npc_marker_*` textures, or `materials/minimap/*` (the pak has `materials/minimap/trooper_test.vmat_c`, `neutral_*.vtex_c`, `shop.vtex_c`), and whether ziplines are textured or drawn as lines, is unknown. [T]

### 3.7 Properties

Seen in vanilla CSS: `wash-color`, `background-color`, `img-shadow`, `brightness` (785 uses across styles), `saturation` (186), `hue-rotation` (3), `contrast` (1), `opacity-brush`. [V grep]

- `background-color`: exact fill. Use for the hero disc.
- `wash-color`: blends a colour over the panel and its children, alpha sets the weight. Right for textured icons (objectives, powerups, arrows). Applied to a whole `.map_button.player` it would also tint the hero portrait. [I, from Valve's Panorama property docs]
- `hue-rotation`, `saturation`, `brightness`: shift a texture's own colours; useful when the texture already has a team colour baked in (arrows, `npc_marker_*`). [I]
- `img-shadow`: glow or outline; can add a contrast ring without changing the fill. [I]

## 4. How DeadTune would do it

The pipeline already handles another stylesheet. `layout::compile` copies each `extra_css` entry into `StylePatch.files` keyed by path, and `install::build_addon` reads each path from the game's `pak01_dir.vpk` and appends with `resource::append_style`. The test `extra_css_appended_per_file` covers a second path. The conflict scan works per path, so QoL Lite and QoL Lock (both ship `hud_minimap.vcss_c`) will be flagged. [V layout.rs:84-112, install.rs:218-275]

Proposed shape: a `minimap_colors` section in `HudLayout`, or a `MINIMAP_COLORS` table like `hud::elements::ELEMENTS`, where each row is (id, selector, property, vanilla value), and `compile` emits into `panorama/styles/hud_minimap.vcss_c`. The user picks colours only; selectors and properties come from the table.

Example rule set (enemy heroes cyan, enemy objectives cyan, allies green):

```css
#hud_minimap .map_button.player.enemy #BackgroundImage { background-color: #00D5FF; }
#hud_minimap .map_button.player.friend #BackgroundImage { background-color: #7CFF6B; }
#hud_minimap .map_button.player.enemy #ArrowImage { wash-color: #00D5FF; }
#hud_minimap .map_button.player.friend #ArrowImage { wash-color: #7CFF6B; }
#hud_minimap .map_button.enemy.boss .boss_image { wash-color: #00D5FF; }
#hud_minimap .map_button.idol_return_enemy .idolReturnTarget { wash-color: #00D5FF; }
```

Specificity: the `#hud_minimap` prefix adds one id, so each rule beats its vanilla counterpart (for example `.enemy.player #BackgroundImage` is 1 id + 2 classes, ours is 2 ids + 3 classes) and does not depend on append order. It also beats the `.player.team1/team2` rules, which matters only for spectators. The localplayer has `friend`, so the ally rule recolours the self disc too; add `:not(.localplayer)` or a separate `.localplayer` row if that is unwanted. [I; CSS specificity rules, Panorama assumed to follow them]

Leave allied objectives on their lane colours unless the user asks; recolouring them hides the lane cue.

Implemented (experimental) in `crates/dt-core/src/hud/minimap_colors.rs`: 28 rows, the hero and objective rows above plus the panel icons of 3.5 (urn spawn, carried urn, allied urn drop-off, camps, vaults, mid boss, five powerups, gold crate spawn, unsecured souls, rejuv crystal, shop, broker, teleporters, stairs). Every non-hero selector matches a rule in the 2026-10-03 vanilla text. [V by grep] The camp and vault rows tint textures with strong baked colour, so how readable they are is [T].

## 5. Competitive information and the denylist

- Recolouring icons the game already shows, keyed on team or type classes (`friend`, `enemy`, `team1`, `team2`, `player`, `boss`, `localplayer`), shows no new information. It is the same thing the official Enemy UI Color setting does. No denylist needed. [I]
- The ConVar denylist (`denylist = true` in `catalog/hud.toml`, 16 entries) does not cover CSS. There is no CSS denylist in `hud::layout`. [V by grep]
- Two ways a colour feature could leak information, which the table design must avoid:
  1. **State classes.** Game code may put a class on an enemy panel that vanilla styles identically to the normal state (for example `lowHealth` exists on `.button_fill` and vanilla animates it only for `.standup.player`). A colour keyed on such a class would reveal state the vanilla UI does not show. Rule: the colour table uses team and type classes only. [I]
  2. **Visibility overrides.** `.modifier_state_no_minimap { visibility: collapse; }`, `.map_button.hide_button`, `.teleporter_off`, `#MinimapEffects` hide things on purpose. A rule that sets `visibility`, `opacity` or `width/height` on those could show what the game hides. The colour feature emits colour properties only. [I]
- `extra_css` stays the free-text escape hatch and can do both of the above. Consider a check in `layout::compile` that rejects `visibility`, `opacity`, `width` and `height` on selectors containing `modifier_state_no_minimap`, `hide_button` or `teleporter_off`. [I, suggestion]
- Brighter or high-contrast enemy colours make enemies easier to spot. That is the purpose of the official setting, so treat it as accessibility. [I]

## 6. Risks

| Risk | Detail | Tag |
|---|---|---|
| Inline style from the enemy-colour setting | With `citadel_custom_ui_colors 1`, game code may set enemy colours inline and override our CSS. | [I, T] |
| Game code sets colours at runtime | Same issue for any icon whose colour is set from game code instead of CSS. The CSS rules in 3.3-3.5 suggest CSS owns these colours. | [I, T] |
| File conflict | QoL Lite and QoL Lock both replace `hud_minimap.vcss_c`. Only one addon's copy can win. Existing conflict scan reports it. | [V scan code; U which wins] |
| Patch drift | Valve renames classes or moves rules. We rebuild from the live file on `buildid` change, but a renamed class makes our rules silently do nothing. Check the table selectors against the live text at build time and warn on misses. | [I] |
| Ally lane cue lost | Recolouring allied objectives drops the lane colours. | [V lane rules exist] |
| Readability | A wash on a textured icon can muddy it; `background-color` on the hero disc is safe. | [I] |
| Texture route | Trooper and zipline recolour needs `.vtex_c` output. DeadTune has no texture encoder. Options: ship a few prebuilt `.vtex_c` per colour (compiled once with Valve's resourcecompiler on Windows) or write an encoder that clones a game `.vtex_c` header and writes uncompressed RGBA. Both are new work and a Valve texture format change breaks them. | [I] |
| Matchmaking | Same as the rest of the HUD addon: nothing found against addon VPKs. | [I, plan H0 step 4] |

## 7. In-game tests

1. Append the example rules to `hud_minimap.vcss_c` with `hud_build`. Do enemy and ally hero discs, enemy objectives and edge arrows change colour? Does the self disc change too?
2. Do the rules hold across states: TAB big map (`gScoreboardOpen`), detail view, death replay, spectating, hideout?
3. Set `citadel_custom_ui_colors 1` and `citadel_enemy_ui_color_r/g/b 0 213 255` with no addon. Does the minimap enemy colour change (heroes, objectives, troopers)? Then load the addon. Which wins?
4. Are those four ConVars live from the console, and are they saved to the per-user config?
5. `#hud_minimap #canvas { wash-color: #00FF00; }`: confirm troopers and ziplines are on the canvas (they turn green, hero panels do not).
6. Swap `panorama/images/minimap/npc_marker_enemy_psd.vtex_c` for a recoloured copy (QoL Lite's approach). Do enemy troopers change? If not, try the `materials/minimap/*` textures.
7. Are ziplines textured (`zip_line_marker_*`) or drawn as lines? Does `citadel_minimap_zip_line_thickness` change them live?
8. With QoL Lite installed alongside, which `hud_minimap.vcss_c` wins (pak number order)? Same as plan H0 step 3.
