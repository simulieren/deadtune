# Deadlock HUD editing: web research for DeadTune (2026-10-04)

Tags: [V] verified, I read the source this session. [I] inferred from sources, not directly stated. [U] unknown / not found.

Local working copies used for verification (not in the repo): clones of ValveResourceFormat (HEAD 2026-10-03) and deadlock-modbox (HEAD 2026-10-03) and the full GameTracking-Deadlock tree listing, all under the scratchpad directory.

## 0. Short answer

- HUD mods are Panorama XML/CSS/JS overrides, compiled to `_c` files and packed into a `pakNN_dir.vpk` in `game/citadel/addons`, mounted by `Game citadel/addons/...` lines in `gameinfo.gi`. [V]
- Everyone who ships HUD mods compiles with Valve's `resourcecompiler.exe` from the community "Reduced CSDK 12" (Windows binary; modbox runs it under Wine). No open-source compiler exists. [V]
- ValveResourceFormat (C#) can re-serialize a `.vcss_c` DATA block, but it is marked "NOT PRODUCTION READY", does no CSS/XML compilation, and `.vxml_c` also needs a `LaCo` AST block that VRF can read but not generate. [V]
- Best product path for DeadTune: do not compile. Edit values in a small set of CSS files by patching, then either (a) call a locally installed resourcecompiler, or (b) ship prebuilt `.vcss_c` templates with numeric slots patched in binary-safe fashion. Option (b) is feasible for CSS only; see section 1.5. [I]
- Matchmaking block is documented for `ConVars` in `gameinfo.gi` and `-tools`. Nothing found that blocks addon VPKs or `SearchPaths`. Valve has made no public statement on HUD mods. [V for the string and the absence of a statement; I for "addon VPKs are not blocked"]
- Valve ships many HUD settings natively (see 3.3). A tool should not duplicate those.

## 1. How Deadlock HUD mods work

### 1.1 Layout and loading

- The shipped HUD source is in `game/citadel/pak01_dir/panorama/{layout,styles,scripts,images}`. `GameTracking-Deadlock` tracks it as decompiled text (the files say "xml reconstructed by Source 2 Viewer"). [V]
  - https://github.com/SteamTracking/GameTracking-Deadlock (last commit read: 2026-10-03T00:53:23Z)
- Vanilla `gameinfo.gi` SearchPaths contain only `Game_UILanguage citadel_*LANGUAGE*`, `Game_LowViolence citadel_lv`, `Game citadel`, `Game core`. Mods add lines. [V]
  - https://raw.githubusercontent.com/SteamTracking/GameTracking-Deadlock/master/game/citadel/gameinfo.gi
- Install convention: create `game/citadel/addons`, drop `pak##_dir.vpk` (01-99, number controls load order), add search path entries to `gameinfo.gi`. [V from search-result summaries of mod pages; not checked against a primary Valve doc]
  - https://deadlocker.net/install-guide
  - Threat HUD README: "copies the bundled `pak57_dir.vpk` into Deadlock's `addons` directory. Activation adds managed `citadel/addons` search-path entries to `gameinfo.gi` while Deadlock is closed." [V] https://github.com/antonborndev/deadlock-threat-hud
- A game update resets `gameinfo.gi`, which disables mods until the search paths are re-added. [V] https://steamcommunity.com/app/1422450/discussions/0/689746131737926232/
- Only compiled `_c` files load in game. Raw `.css/.xml` never do. [V] modbox `nix/README.md`: https://github.com/rszyma/deadlock-modbox
- Mod files shadow vanilla by path. A HUD mod replaces whole files (for example `panorama/styles/hud.css` -> `hud.vcss_c`). Two mods touching the same file collide; modbox merges via patches and fails the build on overlapping hunks. [V]
- Valve changes UI files often; patches can break mods. [V] https://deadlock.one/en/news/deadlock-minor-update-new-item-icons-and-viewsteamprofile-crash-fix (search snippet only) and the Steam thread above.

### 1.2 How modders compile

Toolchain, in order of prevalence:

1. Reduced CSDK 12 (community-merged CS2 Workshop Tools files): `game/bin_tools/win64/resourcecompiler.exe` and `CSDKCfgVPK.exe`. Addon layout: `content/citadel_addons/<addon>/panorama/{layout,styles}/*` as input, `game/citadel_addons/<addon>/...` gets the `_c` output. [V]
   - https://deadlockmodding.pages.dev/modding-tools/csdk-12
   - https://github.com/mntbliss/Deadlock-QoL-HUD-Mod (needs Reduced CSDK 12; calls `resourcecompiler.exe` then `CSDKCfgVPK.exe`, autodetects paths, `paths.json` override)
   - https://github.com/civo7/QOLLOCK (build_mod.bat/.ps1; compiles svg to `.vsvg_c`)
   - Download is a Google Drive zip pinned by hash in modbox `nix/packages/deadlock-csdk.nix`; there is also https://github.com/2Lomi/Reduced_CSDK_12_Bootstrap. [V for modbox, search result only for Bootstrap]
   - Licensing of redistributing Valve's compiler binary: [U]. Do not bundle it.
2. Linux: modbox runs `resourcecompiler.exe` under Wine, headless, cwd in `modroot/game`, flags `-v -f -game <path to game/citadel> <content file>`. It needs `game/core/panorama/panorama_config.txt` copied from the CSDK, and uses `CSDK_BIN=bin_cs2` ("the only flavor that passes the schema check"). Packs with the Python `vpk` package. [V] https://github.com/rszyma/deadlock-modbox (`tools/build-phase.sh`, `nix/README.md`)
3. DeadPacker (TOML-driven compile -> pack -> copy -> relaunch). Calls resourcecompiler. [V] https://github.com/Artemon121/DeadPacker
4. Decompile for reading: Source2Viewer / `Source2Viewer-CLI -i pak01_dir.vpk -e "vjs_c,vxml_c,vcss_c" -o out -d`. [V] https://s2v.app/ValveResourceFormat/guides/command-line.html (via search result)
   - Decompiled CSS is "prettified minified text", XML is a "structural decompile". [V] `docs/guides/format-support.md` in ValveResourceFormat.
5. Deadlock Mod Manager and Grimoire are install managers, not HUD compilers. [I]

### 1.3 Is there an open-source compiler or writer?

- ValveResourceFormat (C#, https://github.com/ValveResourceFormat/ValveResourceFormat): `Resource.Serialize(Stream)` exists. Doc comment: "NOT PRODUCTION READY! Not all blocks support serialization and will throw." Supported: binary KV3, plaintext, Panorama, external reference lists, raw binary. [V] (`ValveResourceFormat/Resource/Resource.cs` L324-330, `docs/guides/format-support.md` L117)
- There is no CSS or XML compiler in VRF. `Panorama` has a constructor `Panorama(byte[] data, List<ImageEntry> images)` that computes CRC32 of the data, but nothing that produces the compiled text from source. Round-trip tests exist for vxml_c and vcss_c (`Tests/Resources/ResourceWriteTest.cs`). [V]
- Rust: no crate parses or writes Source 2 resource files that I could find. `vformats` (0.1.0, 2026-07-26) covers Source 1 formats (KeyValues, VTF, MDL, BSP, VPK). Searches for a Rust vrf/vcss_c crate found nothing. [V for vformats and absence in search results; absence is only as good as the search]
  - https://docs.rs/vformats/latest/vformats/

### 1.4 Compiled resource format (from VRF source)

All offsets from `ValveResourceFormat/Resource/Resource.cs` (Serialize at L329-421) and `Resource/ResourceTypes/Panorama.cs`. [V]

File header (little endian):

| Offset | Size | Field |
|---|---|---|
| 0 | u32 | file size |
| 4 | u16 | header version, always 12 (`KnownHeaderVersion`) |
| 6 | u16 | resource version (Panorama: 2 and 3 seen; image CRC present from 3) |
| 8 | u32 | block table offset, relative to this field, "basically always 8" |
| 12 | u32 | block count |

Both header-version and resource-version are `ushort` in VRF (`Resource.cs` L20, L44, L49). [V]

Block table, 12 bytes each: 4-byte ASCII type (`RED2`, `DATA`, `LaCo`, `SrMa`, `STAT` etc., little-endian packed chars), u32 offset (relative to the position of this offset field), u32 size. Block data is 16-byte aligned; VRF pads with zeros and sometimes the string "S2V" (cosmetic).

Block types relevant to Panorama:

- `RED2` (ResourceEditInfo2): KV3 binary with source file, subasset references and definitions. Serialize just re-emits the parsed KV3 (`BackingData?.Serialize`). Legacy `REDI` is copied raw. Contains no hash that VRF checks. [V]
- `DATA` (class `Panorama`), non-plaintext layout:
  - u32 CRC32 of the payload (IEEE, `System.IO.Hashing.Crc32`)
  - u16 image count, then per image: NUL-terminated UTF-8 path, u16 width, u16 height, and (resource version >= 3) u32 CRC32 of the image file
  - then the payload: UTF-8 text (CSS text, layout text, JS text)
  - Plaintext (no CRC/image header): `PanoramaScript` at version >= 4 and `PanoramaTypescript` at version >= 2.
  - On read VRF throws if CRC32 mismatches, except when the file has an `SrMa` block ("Valve seemingly screwed up when they started minifying vcss and the crc no longer matches"). So CRC32 over the payload is the only hash in DATA, and newer minified vcss may not satisfy it. [V]
- `LaCo` (layout content): KV3 holding the XML AST (`m_AST.m_pRoot`, node `eType` in ROOT/STYLES/INCLUDE/PANEL/SCRIPT_BODY/SCRIPTS/SNIPPET/SNIPPETS). VRF only reads this to print XML (`PanoramaLayout.cs`). Generating this from XML is what resourcecompiler does for `.vxml_c`. [V]
- `SrMa` (source map): KV3 key `DBITSLC` marks a minified vcss with a source map. VRF can prettify but the source-map decode is disabled (`#if false`). [V] (`PanoramaStyle.cs`)
- The game's reader for these files is closed. Whether the engine validates the CRC, the `RED2` contents or the image dimension table at load time is [U].

### 1.5 What this means for DeadTune

- Do not try to write `.vxml_c` ourselves. The AST block (`LaCo`) is a KV3 tree in a Valve-defined schema. [I]
- A `.vcss_c` DATA payload is plain text. In principle you can take the shipped `hud.vcss_c`, replace values in the CSS text (for example `#minimap_persp { width: 440px; ... }`), recompute CRC32, rewrite the DATA block and block table, and keep RED2/SrMa as is. That is a Rust job of roughly 200 lines. Risks: [I]
  - SrMa source map becomes stale (cosmetic if the engine ignores it; [U]).
  - The shipped minified text may not match CRC32 anyway (VRF comment), which suggests the engine does not enforce CRC32 on vcss. [I]
  - A patched vanilla file must be regenerated after every Valve update that touches that file; keep a "base build id" and refuse to apply on mismatch.
- Many Deadlock layout values live in CSS (sizes, margins, alignment, opacity, `visibility: collapse`), so CSS-only patching covers: minimap size/position, health bar placement, hiding elements, top bar scale. Rearranging panels (layout XML changes) needs the real compiler. [I]
- Fallback that is robust: detect a local Reduced CSDK `resourcecompiler.exe` and drive it the way modbox does. Windows direct, Linux via Wine. [I]
- If shipping a binary, never redistribute Valve's CSDK. [I, licence unverified]

## 2. Rust crates for VPK

Registry data from the crates.io API on 2026-10-04 [V]; read/write claims from READMEs/doc pages [V] unless noted.

| Crate | Version | crates.io updated | Read | Write | Notes |
|---|---|---|---|---|---|
| `valve_pak` | 0.1.0 | 2025-06-30 per API (docs.rs page says 0.1.0 released Aug 2026; mismatch unresolved) | v1 + v2, MD5 check on v2 | `from_directory()` / CLI `pack` | Repo floydya/valve-pak-rs. Roadmap lists "v2 write support, checksum validation, MD5 sections, signatures, multi-archive splitting" as planned, so v2 write is not confirmed. 1.2k downloads, young. https://crates.io/crates/valve_pak |
| `sourcepak` | 0.3.0 | 2024-06-22 | v1 and Respawn VPK; "VPK v2 support is limited" | yes (read/write/pack/extract) | Repo barnabwhy/sourcepak-rs. ~6.9k downloads. https://lib.rs/crates/sourcepak |
| `vpk` | 0.3.0 | 2025-03-04 | v1/v2 parser | none (parser only) | roman901/vpk-rs, ~12k downloads. https://github.com/roman901/vpk-rs |
| `rvpk` | 1.1.0 | 2023-03-05 | v1 and v2 extract | create v1 only | panzi/rust-vpk, stale. |
| `vformats` | 0.1.0 | 2026-07-26 | has a `vpk` module (Source 1 suite) | unknown [U] | Very new, ~100 downloads. |

Recommendation [I]: for addons you need a small write path, and Source 2 loads `pakNN_dir.vpk` single-file VPKs fine. A VPK v2 writer for a handful of files is about 150 lines (tree header, per-file CRC32, MD5 sections can be zeroed or computed). Either use `vpk` for reading the base game and hand-write the output, or vet `valve_pak` output by loading it in Source2Viewer and in-game. modbox uses the Python `vpk` package for writing, which is evidence that an uncomplicated v1/v2 file works in Deadlock. [V that modbox uses it; I that it proves v2]. Source 2 Viewer can also create a VPK (UI "rudimentary"). [V] `docs/guides/vpk-management.md`.

## 3. Valve policy and enforcement

### 3.1 Matchmaking restriction

- Shipped string: "Unable to enter matchmaking while any party member has changes to ConVars in gameinfo.gi or is running Tools-mode." Reported by players from 2026-03-26. [V]
  - https://forums.playdeadlock.com/threads/valve-please-optimize-the-game-before-you-disallow-configs.122053/
  - https://x.com/deadlock_8/status/2037230489602314544 (could not fetch, HTTP 402; text known from search snippet) [I]
- Scope: the restriction names ConVars changes and `-tools`. A search snippet and community reply say that `SearchPaths` edits are still fine; not confirmed by Valve. The forum thread itself does not address addon VPKs or HUD mods. [V for what the thread says; I for the conclusion]
- Stated reason (community, not Valve): a response to wallhack/veil exploits and HUD/healthbar ConVar abuse. [V as community statement]
- Sqooky/OptimizationLock still maintains ConVar presets as of the PLAN.md notes (2026-10-03). [V per PLAN.md, not re-fetched]
- The ConVar dump has no `sv_pure` or `sv_consistency` entry (grep of `DumpSource2/convars.txt`, 3 lines match "pure" case-insensitively, none are a pure/consistency check). One hidden devonly var, `panorama_debug_treat_all_addons_as_untrusted`, exists: Panorama has an addon trust concept, effect unknown. [V for the grep, U for meaning]

### 3.2 Bans and Valve statements

- No official Valve statement on HUD mods or addons. A Steam thread says "All mods that give competetive advantage can get you banned, but we have yet to see the first one," and lists "improved HUDs to show item purchases", cooldown timers, enemy minion health through walls as problematic. [V] https://steamcommunity.com/app/1422450/discussions/0/581678423012842506/ (community opinion, posted Aug 1; year not shown)
- A summary on deadlocker.net says mods are allowed in matches and do not get you banned as long as they give no unfair advantage. That is a third-party site, not Valve. [V as a claim, weak source] https://deadlocker.net/install-guide
- Players broke after patches (gameinfo.gi reset, UI file changes); no deliberate Valve action against HUD mods was found. [V for absence in what I read; not exhaustive]
- Practical rule for the app: no HUD changes that expose hidden information (enemy items, timers, through-wall info), keep the existing denylist, offer "Remove all HUD mods / vanilla" in one click. [I]

### 3.3 Official in-game HUD customization (City Never Sleeps update, 2026-09-29)

From https://deadlock.one/en/news/deadlock-city-never-sleeps-update-6-new-heroes-broker-map-overhaul-hud-more [V]:

- Health bars: numeric health inside bars, option to show allies' numeric health when damaged.
- Damage display: more options in Settings including cumulative damage.
- Reticle: separate fill and outline colours, reticle share code, previews.
- Accessibility: "Enemy UI Color", subtitles size/colour/background/font, mouse cursor size.
- Minimap: lane and district labels, enemy clustering. No minimap scale/position option listed.
- Settings: shareable settings export/import, per-hero settings, defaults indicator.
- Inset HUD option for 21:9 displays (earlier patch, secondary source via search result).
- No HUD layout editor, no HUD scale slider, no minimap size or position setting is listed. [V that none appear in these notes; I that none exist]

The matching convars exist in the dump: `citadel_21x9_clamp_hud`, `citadel_enemy_ui_color_{r,g,b}`, `citadel_custom_ui_colors`, `citadel_mouse_cursor_scale`, `citadel_damage_text_cumulative_mode`, `citadel_unit_status_show_*`. [V]

## 4. HUD ConVars

Source: `DumpSource2/convars.txt` in GameTracking-Deadlock, 14,598 lines, current to 2026-10-03. Flag legend as printed in the dump: `developmentonly` = unavailable in release builds (set via `gameinfo.gi` only, which now blocks matchmaking), `cheat` = needs cheats, `release` = FCVAR_RELEASE, usable in release builds, `archive` = saved to config, `clientdll` = client side. My reading of `release`/`archive` semantics is [I].

Usable from the console in a normal game (client, no `developmentonly`, no `cheat`):

| ConVar | Default | Flags | Meaning |
|---|---|---|---|
| `citadel_hud_visible` | true | clientdll release | show/hide whole HUD |
| `citadel_hud_hide_own_health` | false | clientdll release | |
| `citadel_hud_active_stats_always_show_details` | false | clientdll archive | |
| `citadel_always_show_active_hud_stats` | false | clientdll archive | |
| `citadel_21x9_clamp_hud` | true | clientdll archive per_user | inset HUD for ultrawide |
| `citadel_minimap_player_width` | 7 | clientdll release | player icon size |
| `citadel_minimap_local_player_width` | 12 | clientdll release | |
| `citadel_minimap_trooper_size` | 3 | clientdll release | |
| `citadel_minimap_koth_trooper_size` | 4.5 | clientdll release | |
| `citadel_minimap_max_icon_shrink` | 0.8 | clientdll release | |
| `citadel_minimap_overlap_scan_distance` | 12.5 | clientdll release | |
| `citadel_minimap_unit_click_radius` | 200 | clientdll release | |
| `citadel_minimap_zip_line_thickness` / `_hover_radius` / `_hover_thickness_scale` | 2 / 16 / 2 | clientdll release | |
| `citadel_crosshair_hit_marker_duration` | 0.1 | clientdll archive | |
| `citadel_crosshair_enable_clip_status` | false | clientdll release | |
| `citadel_damage_text_cumulative_mode` | 0 | clientdll archive userinfo | |
| `citadel_damage_offscreen_indicator_disabled` | true | clientdll release | |
| `citadel_unit_status_hero_name_mode` | 0 (0-2) | archive per_user release | |
| `citadel_unit_status_show_ally_health_value` / `_show_health_value` / `_show_critical_state` | false/true/true | archive | health bar text |
| `citadel_enemy_ui_color_{r,g,b}`, `citadel_custom_ui_colors` | 255/229/91 (r,g,b of damage delta colour is a different var) | archive per_user | enemy UI colour |
| `citadel_mouse_cursor_scale` | 1 | archive per_user | |
| `deadlock_chat_mode` | 2 | clientdll archive release | |
| `citadel_show_chat_wheel_time` | 0.23 | clientdll archive | |
| `citadel_combat_log_show_damage` | true | clientdll archive release | |
| `cl_hud_telemetry_*` (ping, frametime, net graphs) | various | archive release | network/fps HUD readouts |
| `panorama_debugger_theme`, `panorama_debug_overlay_opacity*`, `panorama_console_position_and_size`, `panorama_toggledebugger_mode` | | hidden / archive | Panorama debugger only |

Cheat-gated (only with cheats, so not in matchmaking): `hud_damagemeter` (DPS next to crosshair), `citadel_damage_text_show_effectiveness`, `citadel_show_movespeed_on_hud`, `citadel_hud_exclusive_visible_id`, `citadel_unit_status_enabled` and `citadel_unit_status_hide_names` (cheat release). [V from dump]

`developmentonly` (gameinfo.gi only, so affected by the matchmaking restriction): all `citadel_crosshair_*` styling (colour r/g/b, pip gap/height/opacity/outline, dot size/opacity/outline, clip angle). In-game reticle settings now cover colours officially (3.3). 85 lines in the dump matching hud/minimap/crosshair/reticle are developmentonly. [V counts by grep, not categorised further]

Not found in the dump: any HUD scale, minimap scale, minimap size or minimap position ConVar. No `killfeed`/`kill_feed`/`datafeed` ConVars; the feed is `HudDataFeed` in `hud.xml`. [V by grep of the file; the grep pattern list was broad but not exhaustive]

Panorama debugger: `panorama_toggledebugger_mode`, `panorama_debugger_theme` exist (hidden). A community wiki also lists `citadel_hud_visible` and `hud_damagemeter`. https://deadlock.wiki/Console_commands [V]

Implication [I]: a "live" HUD tweak panel can set the 20-odd release-flag convars through the existing exec/netcon bridge. Minimap size/position and anything layout related can only come from a Panorama override and needs a restart or at least a map reload (assumption; whether Panorama reloads files live is [U]).

## 5. Panorama files for HUD pieces

All paths under `game/citadel/pak01_dir/panorama/` in https://github.com/SteamTracking/GameTracking-Deadlock (checked via the git tree API, 1,110 panorama paths, 324 with "hud" in the name). [V]

Root: `layout/hud.xml` (373 lines) builds `<CitadelHud class="WindowRoot">` -> `HudCore`. Key child panels (id -> element) [V, from `hud.xml`]:

| HUD piece | Panel id / element | Layout file | Style file |
|---|---|---|---|
| Top bar (portraits, score, timer, midboss timer) | `<CitadelHudTopBar id="TopBar">` with `GradientBacker`, `GameTime`, `TeamScoreFriendly/Enemy`, `TeamsContainer` (`CitadelHudTopBarTeam id=TeamFriendly/TeamEnemy`), `ObjectivesMap`, `RejuvenatorCharges`, `MidbossTimerLabel`, `Team1Chat`/`Team2Chat` | `citadel_hud_top_bar.xml`, `_top_bar_team.xml`, `_top_bar_player.xml`, `_top_bar_player_details.xml`, `_top_bar_chat.xml` | `styles/citadel_hud_top_bar.css`, `citadel_hud_top_bar_player_details.css`, `citadel_hud_top_bar_chat.css` (top bar root: `vertical-align: top; horizontal-align: center`, defines `@define playerDetailsWidth: 88px; playerContainerWidth: 120px;`) |
| Minimap | `#minimap_persp` (440x520, `horizontal-align: right; vertical-align: bottom; margin-bottom: 15px; perspective: 900`) > `#minimap_container` (400x400, `margin-top: 80px`) > `#HudMinimapContainer` (105%) > `<HudMinimap id="hud_minimap">`; also `minimap_frame`, `minimap_blur`, `minimap_hints`, `minimap_location`, `MinimapRevealNotif` | `hud_minimap.xml` (snippets for player icons, cone, crates, pickups) | rules in `styles/hud.css` (~L1842-1960) and `styles/hud_minimap.css` |
| Abilities (4 signature) | `<CitadelHudAbilities id="hud_signature" slots="signature_1 ... signature_4">` inside `#AbilitiesContainer` | `hud_abilities.xml`, `hud_abilities_entry.xml`, `hud_ability_icon*.xml`, `citadel_hud_ability_button.xml`, `ability_panel.xml` | `hud_abilities.css`, `hud_ability_icon*.css`, `ability_panel.css` |
| Item slots (active) | `<ActiveAbilitiesMenu id="ActiveAbilitiesMenu" slots="item_1 item_2 item_3 item_4">`, `CosmeticAbilitiesMenu` (`cosmetic_1`), `HeldAbilitiesMenu` (`ability_held`), `hud_passive_items` (`CitadelHudPassiveAbilities`) | same family | `hud.css` |
| Purchased mods panel | `CitadelModsPurchasedPanel` ids `ModPurchasedPanelUniversal`, `...Locked` inside `StatsAndModsContainer > LowerLeft > ModsContainer` | `hud_scoreboard_mods.xml`, `citadel_hud_active_mods.xml` | `hud_scoreboard_mods.css` |
| Health bar and ability container | `<CitadelHudHealthContainer id="health_and_abilities_container">` | `hud_health_container.xml`, `hud_health.xml`, `hud_health_single_bar.xml`, `hud_health_stacked.xml`, `hud_health_pips.xml`, `hud_shields.xml` | `hud_health*.css` |
| Souls / AP | `<CitadelHudSoulAPContainer id="gold_and_ap_container">`, `APContainer` | `hud_gold_and_ap_container.xml` | `hud_gold_and_ap_container.css` |
| Player stats | `hudPlayerStats`, `hudActivePlayerStats` | `citadel_hud_player_stats.xml`, `citadel_hud_active_player_stats.xml` | |
| Crosshair | `<Citadel_AbilityHUDPanels id="crosshair">`, per-hero element files in `layout/ability_hud_elements/*` (`element_gun.xml`, etc.) and `styles/ability_hud_elements/*` (about 80 files incl. per-hero `abilities_<hero>.css`) | `hud_ability_panels_container.xml`, `ability_hud_elements_container.xml` | `hud_ability_panels_container.css`, `element_common.css` |
| Kill / event feed | `<HudDataFeed id="DataFeed" class="clamp_width">` | `hud_data_feed.xml`, `citadel_hud_data_feed_info/_team_event/_boss_killed/_player_level_up/_banned_heroes/_pause_msg.xml` | `hud_data_feed.css` |
| Chat | `chat.xml`, `TeamChatStatus id=teamChatStatus`, `citadel_hud_top_bar_chat.xml`, `citadel_hud_chat_wheel.xml` | | `chat.css` (presumed) |
| Damage numbers / feedback | `CitadelDamageImpact id=damage_impact`, `CitadelHudDamageSummary`, `CitadelHudDamageReport`, `damage_meter` panel | `hud_damage_impact.xml`, `hud_damage_source_impact.xml`, `hud_damage_summary.xml`, `hud_damage_report.xml`, `citadel_hud_damage_feedback_display.xml` | `hud_damage_*.css` |
| Spirit/other | `ability_resource` (`hud_ability_resource.xml`), `hud_castbars.xml`, `hud_modifiers*.xml`, `hud_quickbuy*.xml` | | |
| Unit health bars in world | `CitadelHudUnitIndicatorsV2 id=unit_indicators_v2` | `hud_unit_indicators_v2.xml` | |

mntbliss QoL HUD modifies `hud_hp_bottom_center.css`, `hud_minimap_rounded.css`, `hud_heart_crosshair.css`, `hud_clear_inventory.css` plus `hud_health.xml`, `hud_gold_and_ap_container.xml`, `hud.xml`, `element_gun.xml`. Those names are the mod's own authoring files; shipped counterparts are `hud.css`, `hud_health.css`, etc. [V; mapping is I]

Everything above `DumpSource2/schemas/panorama_content` is the schema of node types (`ELayoutNodeType.h`, `EStyleNodeType.h`), which tells what a LaCo AST can contain. [V that the files exist, not read]

Note: names such as `chat.css`, `hud_ability_resource.css` I listed from path patterns without opening the style file. [I]

## 6. Existing HUD customization tools

| Tool | What it does | UX takeaway |
|---|---|---|
| mntbliss Deadlock-QoL-HUD-Mod | Single `config.json` with feature toggles (`use_character_hp_bar`, `use_minimap_style`, `use_heart_crosshair`, `use_clear_inventory`, `swap_minimap_inventory`) and numbers (`bar_width: 440px`, `bar_height`, `margin_bottom`, `heart_size`, colours, offsets). Scripts (Bun) patch vanilla CSS, compile via CSDK, pack, patch `gameinfo.gi`. Prebuilt `install_compiled.bat` for users without tools. [V] https://github.com/mntbliss/Deadlock-QoL-HUD-Mod | Declarative config -> regenerate. Close to what DeadTune would do with a GUI on top. |
| QOLLOCK (civo7) | Panorama XML/CSS/JS in-game mod with its own settings system and a GUI, an update-checker JS, presets/bindings docs. Also GameBanana "QOL Lock Custom Announcer Maker" tool. [V for repo summary; GUI contents not verified] https://github.com/civo7/QOLLOCK , https://gamebanana.com/tools/22059 | Settings live inside the game as a Panorama page (JS writes values), so no external compile for user changes. Worth studying if DeadTune should install one such in-game settings mod instead of patching CSS. |
| HUDlock 2.0 (GameBanana) | "Monster mega mod mashup" of HUD mods. Page content not retrievable. [V that it exists, U on contents] https://gamebanana.com/mods/629803 | Mashups exist because file collisions are the main pain. |
| Rosey HUD Edits, DEADLOCK TOASTED, HUD/UI collections | Static HUD packs. Pages not retrievable. [U] https://gamebanana.com/mods/658757 | |
| deadlock-modbox (rszyma) | Mods as `.patch` against decompiled vanilla UI, built to one `pak75_dir.vpk`; fails on overlapping hunks; Nix + Wine. Examples: always show ability suggestions, hide upgrade reminder, remove esc menu animations. [V] https://github.com/rszyma/deadlock-modbox | Patch-per-feature with collision detection is a good merge model for stacking HUD tweaks. |
| Deadlock Threat HUD | JS Panorama mod (37 scripts, 3 layouts) plus Windows companion that activates search paths. [V] https://github.com/antonborndev/deadlock-threat-hud | Managed `gameinfo.gi` search-path block activated while the game is closed: matches DeadTune's existing gameinfo editing. |
| DeadPacker | TOML pipeline compile/pack/copy/relaunch. [V] https://github.com/Artemon121/DeadPacker | One-click "build and relaunch" loop. |
| Deadlock Mod Manager / Grimoire | Install, order and toggle VPK mods, manage gameinfo.gi. [I] https://docs.deadlockmods.app | DeadTune's non-goal: coexist. |
| Deadlimit | GUI for model/texture authoring, not HUD. [V] https://github.com/downlimit/Deadlimit | Shows a Manager-style GUI is acceptable. |
| Moonah's Mod Maker | Page not retrievable. [U] https://gamebanana.com/tools/23422 | |

No GUI editor that visually places minimap, top bar or ability slots was found. [V for absence in what I searched]

## 7. Gaps and recommended follow-ups

1. Check on a real install whether a patched `hud.vcss_c` (modified DATA text, recomputed CRC32, original RED2/SrMa) loads. This decides the "no compiler needed for CSS" route. [U]
2. Find out what `panorama_debug_treat_all_addons_as_untrusted` restricts (probably JS API access for addon panels). [U]
3. Open `styles/hud.css` fully and list which of the ~60 rules matter for minimap, abilities, items, top bar; generate the catalog of editable CSS properties from it.
4. Confirm that a mounted `addons` VPK does not trip the matchmaking check in a live test party. [U]
5. Decide on the `valve_pak` vs hand-written VPK writer, test by loading the result with Source2Viewer-CLI and in game.
6. Read QOLLOCK's settings page to see how an in-game Panorama settings UI stores values (could replace a compile step for user changes).
