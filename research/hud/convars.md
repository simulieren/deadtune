# HUD / UI ConVar inventory for DeadTune

Scope: every convar, video.txt key and launch option in the repo research data that changes a HUD or UI element. Compiled from `research/configs/OptimizationLock/cvarlist.txt` (5,872 rows, full dump with flags and defaults), `cvars_we_can_modify.txt` (2,617 rows), `convars.txt`, `launch_options.txt`, `research/configs/OptiLock/cvarlist.md`, `OptiLock/*/video.txt`, `reference/Deadlock-Config/autoexec.cfg`, `catalog/catalog.toml`, `catalog/curated.toml` and `research/data/convar_catalog.csv`.

Method note (what was and was not verified):
- Verified by reading the files: names, flags, defaults and descriptions below come straight from `cvarlist.txt` (defaults were cross-checked against `cvars_we_can_modify.txt` where present). The "In catalog.toml" column was computed by loading `catalog/catalog.toml` with a TOML parser.
- Inferred, not tested in game: the Apply column. It follows the PLAN.md 2.2 rule as implemented in `crates/dt-core/src/catalog.rs` (`devonly` -> restart, else `cheat` -> live_cheat, else live). Nothing was run against Deadlock, so "live" means "console-settable per the flag dump", not "visually confirmed to update the HUD without a reload". Many HUD panels read a convar only when a panel is built, so a live convar may still need a map reload or panel rebuild to show. Phase 0 spike should confirm per element.
- `core.zip` (`_incoming/`): unzipped to the scratchpad. It holds an earlier copy of the same research tree (`deadlock-config-research/`): same cvarlists, configs, reference repos. Nothing extra for HUD. `diff -rq` against `research/` shows only vpk/jpg assets and PLAN/README differences.
- The "spreadsheet" mentioned in the brief is not in the repo: `research/reference/Deadlock-Config/` has only `autoexec.cfg`, README, LICENSE and screenshots. Its autoexec mentions exactly two HUD tweaks (both commented out): `citadel_damage_text_lifetime` and `citadel_crosshair_hit_marker_duration`, with "set to 0 to disable".
- `OptiLock/cvarlist.md` has all `devonly` flags stripped (0 occurrences vs 3,510 in `cvarlist.txt`), so it cannot be used to derive apply classes. Use `cvarlist.txt`.

Flag glossary (as printed in the dump): `devonly` = developer-only, not settable from the release console (gameinfo.gi only); `cheat` = needs sv_cheats; `cl` client; `sv` server; `rep` replicated; `a` archive (saved to config); `release` marked available in release builds; `user`/`per_user` stored per account; `hidden`.

Deny column: `D` = recommend denylist (reveals competitive information, same rule as `curated.toml`), `R` = review (changes what range or detail of enemy info is shown, or could, decide per entry), `D (curated)` = already `denylist = true` in `catalog/curated.toml`, blank = cosmetic or layout only. Entries tagged "(server-side only)" in Apply are `sv` without `cl`/`rep`; the client cannot change them in a match and they are only listed for minimap. These deny tags are my judgement, not Valve policy.

## Summary of what exists

| HUD element | Convars that exist | Customisable? |
|---|---|---|
| Crosshair | Color (RGB), pip gap/height/width/opacity, center dot size/opacity, outlines, hit marker duration, hero-specific crosshair disable | Yes, best-covered element. All `cl, a` plain flags, so apply class live. |
| Minimap | Icon sizes (player, local player, trooper, zipline), icon shrink, new vs old minimap | Sizes of icons only. No position, scale, rotation or zoom level convar. |
| Overhead health bars | Master switch, width, height, DPI, fade distances, pips, names, single-bar mode, vertical bars | Yes, but nearly all `devonly` (restart). |
| Top bar | `citadel_show_new_topbar`, dynamic player position, chat top-bar limits | Layout variant toggle only. No score/souls/timer positioning. |
| Ability and item slots | Quickcast, alt-cast, radial menu, shop grouping, active-slot popup | Behaviour, not layout. No slot position or size. |
| Damage numbers | Lifetime, offsets, spacing, batching window, effectiveness text | Yes (devonly, restart). |
| Kill feed | No dedicated convar found. `citadel_show_seasonal_kill_toast`, commend and purchase toasts, announcement timers only. | No |
| Chat | Fade time, max messages, ping indicator sizes, chat mode | Partly (mostly devonly). |
| HUD scale | No global HUD scale convar found. Closest: `mat_viewportscale` (3D render scale), `cl_cursor_scale`, `ui_hud_dist`, `-force_dpi`, `-cursor_scale_percent` launch flags. | No |
| Master visibility | `citadel_hud_visible` (live, no cheat flag), `cl_drawhud` and `r_drawpanorama` (cheat) | Yes |


## Launch options relevant to UI

`launch_options.txt` is a bare list of ~1,500 engine command-line switches with no descriptions, so the "what it does" below is from the switch names and Source 2 conventions; none were tested. Apply class for all launch options: restart (read at process start).

| Switch | Affects | Deny | Note |
|---|---|---|---|
| `-force_dpi` | UI scaling by DPI | | Name suggests it forces the DPI used for UI layout; the only plausible global UI scale lever. Unverified. |
| `-highdpi`, `-no_high_dpi_scaling` | UI/render scaling on high-DPI displays | | Toggle high-DPI behaviour. Unverified. |
| `-cursor_scale_percent` | Mouse cursor size | | Launch-time twin of `cl_cursor_scale`. |
| `-debug_font_size` | Debug font only | | Not gameplay HUD. |
| `-new_default_font` | Default UI font | | Unverified. |
| `-prewarm_panorama`, `-no_prewarm_panorama` | Panorama UI preload | | Startup time and hitching, no visual change. |
| `-panorama`, `-panorama_nosinglecontext` | Panorama context setup | | Internal. |
| `-nopanoramajoy`, `-panoramajoy` | Panorama gamepad navigation | | Input, not visuals. |
| `-gamepadui`, `-nogamepadui` | Big-picture style UI | | Unverified for Deadlock. |
| `-legacyscaleformui`, `-scaleform`, `-no_scaleform_menu_on_boot` | Legacy Scaleform UI | | Leftovers from older Source 1/CS:GO builds; likely no effect in Deadlock. |
| `-vguimessages`, `-vgui_drawtree`, `-vguifocus` | VGUI debug | | Debug only. |
| `-width`, `-height`, `-window`, `-windowed`, `-noborder`, `-fullscreen` | Window size (indirectly HUD pixel size) | | Already handled by video.txt and launch builder. |

## video.txt

`OptiLock/*/video.txt` contains no HUD keys. Relevant adjacent keys only:

| Key | Default in OptiLock Recommended | Effect |
|---|---|---|
| `setting.mat_viewportscale` | 1.000000 | Render resolution scale of the 3D scene. The HUD is composited at native resolution, so lowering it does not shrink the HUD. |
| `setting.r_dashboard_render_quality` | 0 (FPS preset) | Render quality for in-world dashboard panels (devonly cvar). |
| `setting.r_reduce_flash` | 1 | Reduces screen flash effects (accessibility, damage/ability flashes). |
| `setting.aspectratiomode`, `setting.defaultres*`, `setting.high_dpi` | resolution | Changes pixel size of HUD indirectly (HUD scales with resolution). |
| `setting.r_citadel_outlines` | 1 | Outlines (denylisted in curated.toml). |

## What convars cannot do

Based on a full read of the dump (5,872 names) for these terms: minimap, hud, topbar/top_bar, scale, layout, anchor, position, rotation, killfeed, scoreboard, portrait. Absence in the dump is evidence, not proof: the dump is from a specific build and layout lives in Panorama XML/CSS inside VPKs.

- Minimap position, anchor, screen margin, global scale, rotation/north-up, opacity or zoom level: no convar. Only icon sizes, `citadel_use_new_minimap`, `citadel_zoomed_in_minimap` (devonly, described in the dump as a test toggle), and bindable `minimap_zoom_in` / `minimap_zoom_out` commands. Moving or resizing the minimap needs a Panorama layout/CSS override (a VPK mod placed in `citadel/addons` or similar), which is the "mods" route you want to avoid.
- Top bar: no way to move or resize hero portraits, score, souls or timer. Only the `citadel_show_new_topbar` variant switch (devonly, default false) and `citadel_hud_top_bar_enable_dynamic_player_position`.
- Ability and item slot position, size, count or order: none. Only input behaviour (`cl_citadel_quickcast_ability1-4`, `cl_citadel_ability_alt_cast_*`, `cl_citadel_items_quickcast_mode`) and shop grouping.
- Global HUD scale: none. `mat_viewportscale` scales the 3D view only. Windows display scaling and the `-force_dpi` / `-highdpi` launch switches are the only candidates and are untested. Resolution plus aspect ratio is the practical lever.
- Kill feed: no kill feed convars (position, count, duration). Closest are toast timers (`citadel_commend_toast_seconds`, `citadel_commend_toast_enemy_seconds`, `citadel_show_all_purchase_toasts`) and announcement display times.
- Per-hero crosshair shapes: only the global pip/dot parameters and `citadel_crosshair_disable_hero_specific_crosshairs`; no custom crosshair image.
- Health/ammo bar layout of the player's own HUD: no convars. Overhead (world-space) bars of other units are customisable; the bottom-HUD player health and ammo are not.
- Font, colour theme, colour-blind modes for HUD: none found (outside `r_reduce_flash`).
- Anything the server owns: `sv`-only convars (for example `minimap_update_rate_hz`) cannot be changed from the client in matchmaking.
- Panorama layout/CSS is not a convar surface at all: `dump_panorama_css_properties`, `dump_panorama_events`, `panorama_generate_layout_xsd` (all release or devonly) only print schemas. Layout editing means shipping a modified UI VPK.

## How catalog.toml entries are structured (for adding a `hud` category)

File header: `# Generated by cargo run -p dt-core --example gen_catalog. Edit catalog/curated.toml instead.` Top-level table name is the convar name; sorted alphabetically. Real entry from `catalog/catalog.toml`:

```toml
[citadel_damage_text_lifetime]
category = "UI & Panorama"
type = "float"
default = "1.5"
apply = "restart"
impact = "unknown"
denylist = false
notes = "How long do numbers live."

[citadel_damage_text_lifetime.presets]
kaiz_minspec = "0.01"
kaiz_extremelow = "0.01"
boot_maxfps = "0.5"
optilock_recommended = "0.5"
optilock_potato = "0.5"
```

Rust shape (`crates/dt-core/src/catalog.rs`): `CatalogEntry { category: String, #[serde(flatten)] kind: Kind, range: Option<[f64;2]>, step: Option<f64>, default: Option<String>, apply: ApplyClass, impact: Impact, denylist: bool, notes: String, presets: BTreeMap<PresetId,String> }`. `Kind` is internally tagged by `type` = `bool | int | float | enum {options} | string`. `ApplyClass` serialises as `live | live_cheat | restart`. `Impact` as `high | medium | low | unknown`. `default` is always a string. A leading `//` in a preset value means "present but commented out".

Curated overlay (`catalog/curated.toml`): per-convar tables with any of `category, type (+options for enum), range, step, impact, denylist, notes`; each field replaces the generated one. Example:

```toml
[r_citadel_shadow_quality]
range = [0, 3]
step = 1
impact = "high"
notes = "Overall shadow quality level. 0 is lowest; most FPS presets use 0."
```

Findings an implementer needs:

1. `catalog.toml` has 761 entries and is built only from `research/data/convar_catalog.csv`, which contains only convars that some preset touches. Of the 376 HUD convars tabulated in this report (Panorama internals excluded) only 52 are in the catalog. Two more HUD-ish catalog entries, `citadel_minimap_use_canvas_for_neutrals` and `citadel_minimap_use_canvas_for_shop`, are not in the dump at all (apply = restart by the unknown rule). Crosshair, minimap sizes, `cl_drawhud`, `citadel_hud_visible`, `citadel_unit_status_enabled` and `citadel_show_new_topbar` are absent. A `hud` category therefore needs the generator to ingest the full dump (`cvarlist.txt`: name, flags, default, description), not just add a category rule. Apply class must be derived from flags in code (devonly -> restart, cheat -> live_cheat, else live), because the CSV `apply_class` column exists only for preset-touched rows. `apply_class_from_csv` currently maps anything not exactly `live` / `live w/ sv_cheats...` to Restart, which covers unknown too.
2. A `UI & Panorama` category already exists (67 entries) with two rules in `CATEGORY_RULES` (`catalog.rs` ~line 253 and ~343). First match wins. Existing keywords for the second rule: `hud, unit_status, damage, minimap, caption, toast, ping, chat_wheel, crosshair, portrait, survey, dashboard`. Pitfalls: substring match means `ping` hits `shipping`, `mapping`, `damping`, `grouping`; `damage` hits dozens of server balance cvars (`citadel_*_damage_*`); `glow` and `outline` are matched earlier by "Outlines & glow", so `r_citadel_glow_health_bars` lands there, not in UI. Recommend explicit per-name `category = "HUD: Crosshair"` etc. in `curated.toml` (the overlay supports `category`) rather than more substring rules, or at least anchor with `^` / word boundaries.
3. Existing miscategorisation to fix when adding `hud`: `citadel_crosshair_hit_marker_duration`, `citadel_damage_text_batching_window_ability` and `citadel_auto_ping_window` sit in "World detail" (substring `wind` matches `window`), `citadel_enable_new_ping_particle` in "Particles & effects", `citadel_in_world_item_panel_dpi` in "Advanced", `mat_viewportscale` in "Textures & shaders", `r_drawviewmodel` / `viewmodel_fov` in "Camera & view".
3b. Suggested category names matching the groups below: `HUD: Crosshair`, `HUD: Minimap`, `HUD: Health bars`, `HUD: Damage numbers`, `HUD: Chat and pings`, `HUD: Top bar and toasts`, `HUD: Abilities and items`, `HUD: Global`, `HUD: Overlays`, `UI & Panorama` (engine).
4. Types and ranges are not in the dump. Infer from defaults (`true/false` bool, ints, floats) and hand-curate: crosshair color `range = [0, 255]`, step 1; opacities `[0, 1]`, step 0.05; `cl_hud_telemetry_*_show` as `enum` 0/1/2.
5. `denylist` entries in `curated.toml` already cover: `r_citadel_glow_health_bars`, `citadel_unit_status_allies_see_thru_walls(+_max_distance)`, `minimap_update_rate_hz`, `minimap_trooper_update_rate_hz`, `citadel_minimap_overlap_scan_distance`, outlines, PVS entries. Candidates to add are the `D` rows below, notably `citadel_minimap_draw_fow`, `citadel_show_minimap_reveal_indicators`, `citadel_unit_status_use_v2`, `citadel_show_playerintents*`, `cl_ent_show_damage`, spectator minimap fog views.
6. Apply-class caveat for HUD: of the 376 tabulated rows, 245 are `devonly` (restart), 108 live and 23 live_cheat, so most layout and timing tweaks are restart-only (gameinfo.gi `ConVars` block). Live tweaks are concentrated in crosshair, minimap icon sizes, telemetry, captions, cursor and master HUD toggles. Cheat-gated live items (`cl_drawhud`, `r_drawpanorama`, `r_drawviewmodel`, `viewmodel_fov`, `citadel_unit_status_enabled`, `citadel_unit_status_hide_names`, `citadel_unit_status_single_bar_mode`) only work where `sv_cheats` is on (hideout/sandbox).

## Recommended first set for a UI editor (no mods)

Live, safe, no denylist concern (plain flags, `cl, a` or `cl, release`):

1. Crosshair: `citadel_crosshair_color_r/g/b`, `_pip_gap/_height/_width/_opacity`, `_dot_size/_dot_opacity`, `_pip_gap_static`, outline border/gap/opacity/color, `_hit_marker_duration`, `_disable_hero_specific_crosshairs`. Natural live preview target.
2. Minimap icon sizes: `citadel_minimap_player_width`, `_local_player_width`, `_trooper_size`, `_koth_trooper_size`, `_max_icon_shrink`, `_zip_line_thickness`.
3. Master toggles: `citadel_hud_visible`, `citadel_healthbars_enabled` (R), cursor scale, `cl_hud_telemetry_*` (ping/frametime/net readouts), `cl_showfps`, `closecaption`.
4. Behaviour: `cl_citadel_quickcast_ability1-4`, `cl_citadel_ability_alt_cast_*`, `citadel_always_show_active_hud_stats`, `citadel_show_all_purchase_toasts`, `citadel_show_active_slot_popup`.

Restart (gameinfo.gi): damage text group, overhead bar size/DPI, announcement/toast/chat timers, `citadel_show_new_topbar`, `citadel_use_new_minimap`, `citadel_use_vertical_healthbars`.

## Reproducing this report

The tables were generated by a throwaway script from `cvarlist.txt` in the scratchpad (`gen.py`, `all.json`), not committed. If this becomes a feature, the same logic belongs in `gen_catalog` (see finding 1). Candidate skill: none needed yet.

## Tables by HUD element

Columns: Name, Flags (raw from dump), Default, Apply (flag-derived, unverified in game), whether already in `catalog.toml` (with its category), Deny tag, Description (dump text; where the dump is blank, a short note derived from the name, which is not authoritative).

### Crosshair and hit marker

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_crosshair_clip_angle` | devonly, cl | 90 | restart | no |  | (no description in dump) |
| `citadel_crosshair_clip_bullet_gap` | devonly, cl | 0.5 | restart | no |  | (no description in dump) |
| `citadel_crosshair_clip_offset_angle` | devonly, cl | 180 | restart | no |  | (no description in dump) |
| `citadel_crosshair_color_b` | cl, a | 255 | live | no |  | Crosshair blue 0-255. |
| `citadel_crosshair_color_g` | cl, a | 255 | live | no |  | Crosshair green 0-255. |
| `citadel_crosshair_color_r` | cl, a | 255 | live | no |  | Crosshair red 0-255. |
| `citadel_crosshair_disable_hero_specific_crosshairs` | cl, a | false | live | no |  | Force the generic crosshair instead of per-hero ones. |
| `citadel_crosshair_dot_opacity` | cl, a | 0.7 | live | no |  | Center dot opacity. |
| `citadel_crosshair_dot_outline_border` | cl, a | 2 | live | no |  | (no description in dump) |
| `citadel_crosshair_dot_outline_gap` | cl, a | 0 | live | no |  | (no description in dump) |
| `citadel_crosshair_dot_outline_opacity` | cl, a | 0.7 | live | no |  | (no description in dump) |
| `citadel_crosshair_dot_size` | cl, a | 4 | live | no |  | Center dot size. |
| `citadel_crosshair_hit_marker_duration` | cl, a | 0.1 | live | yes: World detail |  | Hit marker lifetime; 0 disables (autoexec preset comment). |
| `citadel_crosshair_out_of_range_dist` | devonly, sv, cl, rep | 50 | restart | no |  | (no description in dump) |
| `citadel_crosshair_outline_color_b` | cl, a | 0 | live | no |  | (no description in dump) |
| `citadel_crosshair_outline_color_g` | cl, a | 0 | live | no |  | (no description in dump) |
| `citadel_crosshair_outline_color_r` | cl, a | 0 | live | no |  | (no description in dump) |
| `citadel_crosshair_pip_gap` | cl, a | 4 | live | no |  | Gap between pips and center. |
| `citadel_crosshair_pip_gap_static` | cl, a | false | live | no |  | Pips do not spread with movement/recoil. |
| `citadel_crosshair_pip_height` | cl, a | 16 | live | no |  | Pip length. |
| `citadel_crosshair_pip_opacity` | cl, a | 0.5 | live | no |  | Pip opacity. |
| `citadel_crosshair_pip_outline_border` | cl, a | 1 | live | no |  | (no description in dump) |
| `citadel_crosshair_pip_outline_gap` | cl, a | 0 | live | no |  | (no description in dump) |
| `citadel_crosshair_pip_outline_opacity` | cl, a | 0.7 | live | no |  | (no description in dump) |
| `citadel_crosshair_pip_width` | cl, a | 2 | live | no |  | Pip thickness. |
| `citadel_use_csgo_style_recoil_follow_crosshair` | devonly, cl, rep | false | restart | no |  | (no description in dump) |
| `crosshair_spread_scale` | devonly, cl | 3.6 | restart | no |  | (no description in dump) |

### Minimap

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_default_minimap_icon_radius` | devonly, cl | 18 | restart | no |  | Default icon radius. |
| `citadel_distance_mouse_move_for_minimap_drawing` | cl, release | 15 | live | yes: UI & Panorama |  | Mouse travel before minimap drawing starts. |
| `citadel_generator_minimap_icon_radius` | devonly, cl | 24 | restart | no |  | Generator icon radius. |
| `citadel_minimap_arrow_show_distance_down` | devonly, sv | 100 | restart (server-side only) | no |  | (no description in dump) |
| `citadel_minimap_arrow_show_distance_up` | devonly, sv | 200 | restart (server-side only) | no |  | (no description in dump) |
| `citadel_minimap_draw_fow` | cl, cheat | false | live_cheat | no | D | Draws fog of war on minimap. |
| `citadel_minimap_koth_trooper_size` | cl, release | 4.5 | live | no |  | Trooper dot size in KOTH mode. |
| `citadel_minimap_local_player_width` | cl, release | 12 | live | no |  | Own arrow width on minimap. |
| `citadel_minimap_max_icon_shrink` | cl, release | 0.8 | live | no |  | Max shrink factor when icons overlap. |
| `citadel_minimap_npc_reveal_duration` | devonly, sv | 0.25 | restart (server-side only) | no | D | (no description in dump) |
| `citadel_minimap_objective_damaged_reveal_duration` | devonly, sv | 1 | restart (server-side only) | no | D | (no description in dump) |
| `citadel_minimap_overlap_scan_distance` | cl, release | 12.5 | live | yes: UI & Panorama | D (curated) | Icon overlap scan distance; already denylisted in curated.toml. |
| `citadel_minimap_player_width` | cl, release | 7 | live | no |  | Player icon width on minimap. |
| `citadel_minimap_show_hitboxes` | devonly, cl | false | restart | no | D | (no description in dump) |
| `citadel_minimap_spectator_fow_team_view` | cl, release | 1 | live | no | D | Spectator-only fog view. |
| `citadel_minimap_teleporter_active_dist` | devonly, cl | 400 | restart | no |  | (no description in dump) |
| `citadel_minimap_teleporter_height_dist` | devonly, cl | 160 | restart | no |  | (no description in dump) |
| `citadel_minimap_teleporter_nearby_dist` | devonly, cl | 1600 | restart | no |  | (no description in dump) |
| `citadel_minimap_trooper_size` | cl, release | 3 | live | no |  | Trooper dot size on minimap. |
| `citadel_minimap_unit_click_radius` | cl, release | 200 | live | no |  | Click-radius for minimap unit selection. |
| `citadel_minimap_use_effects` | devonly, cl | false | restart | no | R | (no description in dump) |
| `citadel_minimap_zip_line_thickness` | cl, release | 2 | live | no |  | Zipline line thickness on minimap. |
| `citadel_render_minimap` | cl, release | - | live | no |  | Command: renders minimap. |
| `citadel_show_minimap_reveal_indicators` | devonly, cl | false | restart | no | D | Shows reveal indicators on minimap. |
| `citadel_tier1_minimap_icon_radius` | devonly, cl | 12 | restart | no |  | Tier-1 objective icon radius. |
| `citadel_use_new_minimap` | devonly, cl | true | restart | no |  | Old vs new minimap implementation. |
| `citadel_zoomed_in_minimap` | devonly, cl | false | restart | no |  | Test zoomed minimap; fixed zoom toggle, not a scale slider. |
| `hud_minimap_spectator_fow_team_view_amber` | cl, release | - | live | no | D | While a spectator, view team amber's minimap view |
| `hud_minimap_spectator_fow_team_view_both_teams` | cl, release | - | live | no | D | While a spectator, view both teams' minimap view |
| `hud_minimap_spectator_fow_team_view_sapphire` | cl, release | - | live | no | D | While a spectator, view team sapphire's minimap view |
| `hud_minimap_spectator_fow_team_view_target_team` | cl, release | - | live | no | D | While a spectator and viewing a player, view team their minimap view |
| `minimap_add_glow_modifier` | devonly, sv | false | restart (server-side only) | no | D | (no description in dump) |
| `minimap_update_rate_hz` | sv, release | 30 | live (server-side only) | yes: UI & Panorama | D (curated) | Server minimap update rate; already denylisted. Server-side. |
| `minimap_zoom_in` | cl, release | - | live | no |  | Bindable command: zoom in. |
| `minimap_zoom_out` | cl, release | - | live | no |  | Bindable command: zoom out. |
| `ping_trace_radius_minimap` | devonly, cl | 60 | restart | no |  | (no description in dump) |

### Overhead health bars / unit status

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_healthbars_enabled` | cl, release | true | live | no | R | Master switch for health bars. |
| `citadel_hud_healthbars_use_new` | cl, cheat, release | false | live_cheat | no |  | Use new HUD health bars. |
| `citadel_hud_healthbars_use_v2` | cl, cheat, release | false | live_cheat | no |  | Use v2 HUD health bars. |
| `citadel_mobile_resupply_healthbar_pos` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `citadel_unit_status_allies_see_thru_walls` | devonly, cl | true | restart | yes: Outlines & glow | D (curated) | Ally unit status through walls; already denylisted. |
| `citadel_unit_status_allies_see_thru_walls_max_distance` | devonly, cl, cheat | 0 | restart | yes: Outlines & glow | D (curated) | Already denylisted. |
| `citadel_unit_status_delta_decay_delay` | devonly, cl | 0.166667 | restart | yes: UI & Panorama |  | (no description in dump) |
| `citadel_unit_status_delta_decay_rate` | devonly, cl | 3 | restart | yes: UI & Panorama |  | (no description in dump) |
| `citadel_unit_status_dpi` | devonly, cl | 10 | restart | yes: UI & Panorama |  | Overhead bar texture resolution. |
| `citadel_unit_status_draw_local` | devonly, cl | false | restart | no | R | (no description in dump) |
| `citadel_unit_status_enabled` | cl, cheat, release | true | live_cheat | no | R | Master switch for overhead unit status bars. |
| `citadel_unit_status_fadeout_dist` | devonly, cl | 200 | restart | no | R | Distance beyond effective gun range where bar fades. |
| `citadel_unit_status_health_per_minor_pip` | devonly, cl | 100 | restart | no |  | (no description in dump) |
| `citadel_unit_status_health_per_pip` | devonly, cl | 100 | restart | no |  | (no description in dump) |
| `citadel_unit_status_health_pips_per_row` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `citadel_unit_status_healthbar_highlight_speed` | devonly, cl | 2 | restart | no |  | (no description in dump) |
| `citadel_unit_status_height` | devonly, cl | 100 | restart | no |  | Overhead bar height. |
| `citadel_unit_status_hide_names` | cl, cheat, release | false | live_cheat | yes: UI & Panorama |  | Hide names on overhead bars. |
| `citadel_unit_status_max_distance_distance` | devonly, cl | 800 | restart | no | R | Distance at which bar scale reaches max-distance scale. |
| `citadel_unit_status_max_distance_scale` | devonly, cl | 1 | restart | no |  | (no description in dump) |
| `citadel_unit_status_max_health_per_bar` | devonly, cl | 1000 | restart | no |  | (no description in dump) |
| `citadel_unit_status_max_health_segment_increment` | devonly, cl | 1000 | restart | no |  | (no description in dump) |
| `citadel_unit_status_max_health_segments` | devonly, cl | 6 | restart | no |  | (no description in dump) |
| `citadel_unit_status_max_total_bars` | devonly, cl | 6 | restart | no |  | (no description in dump) |
| `citadel_unit_status_min_distance_scale` | devonly, cl | 0.2 | restart | no |  | (no description in dump) |
| `citadel_unit_status_minor_pip_per_major_pip` | devonly, cl | 5 | restart | no |  | (no description in dump) |
| `citadel_unit_status_old_dpi` | devonly, cl | 4 | restart | no |  | (no description in dump) |
| `citadel_unit_status_old_draw_local` | devonly, cl | false | restart | no | R | (no description in dump) |
| `citadel_unit_status_old_fadeout_dist` | devonly, cl | 200 | restart | no | R | How far out of the players effective gun range do we show the health bar |
| `citadel_unit_status_old_health_pips_per_row` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `citadel_unit_status_old_height` | devonly, cl | 80 | restart | no |  | (no description in dump) |
| `citadel_unit_status_old_hide_names` | cl, cheat, release | false | live_cheat | no |  | (no description in dump) |
| `citadel_unit_status_old_opaque_dist_sq` | devonly, cl | 50000 | restart | no | R | (no description in dump) |
| `citadel_unit_status_old_show_stats` | devonly, cl | false | restart | no | R | (no description in dump) |
| `citadel_unit_status_old_transparent_dist_sq` | devonly, cl | 0 | restart | no | R | (no description in dump) |
| `citadel_unit_status_old_update_rate` | devonly, cl | 30 | restart | yes: UI & Panorama |  | How many times per second the unit status can update (0 = every frame). |
| `citadel_unit_status_old_width` | devonly, cl | 100 | restart | no |  | (no description in dump) |
| `citadel_unit_status_opaque_dist_sq` | devonly, cl | 50000 | restart | no | R | (no description in dump) |
| `citadel_unit_status_recent_damage_time` | devonly, cl | 0.25 | restart | yes: UI & Panorama |  | (no description in dump) |
| `citadel_unit_status_show_stats` | devonly, cl | false | restart | no | R | Adds stats to overhead bar. |
| `citadel_unit_status_single_bar_mode` | cl, cheat | false | live_cheat | yes: UI & Panorama |  | Single health bar, no stacking. |
| `citadel_unit_status_stamina_consume_linger` | devonly, cl | 3 | restart | no | D | How long enemy stamina-consume marker lingers. |
| `citadel_unit_status_transparent_dist_sq` | devonly, cl | 0 | restart | no | R | (no description in dump) |
| `citadel_unit_status_use_new` | cl, release | false | live | yes: UI & Panorama |  | (no description in dump) |
| `citadel_unit_status_use_v2` | devonly, cl, cheat | false | restart | yes: UI & Panorama | D | New health bar with enemy stamina display. |
| `citadel_unit_status_use_v2_for_nonplayers` | devonly, cl, cheat | false | restart | yes: UI & Panorama | D | (no description in dump) |
| `citadel_unit_status_v2_height` | devonly, cl | 210 | restart | no |  | (no description in dump) |
| `citadel_unit_status_v2_width` | devonly, cl | 200 | restart | no |  | (no description in dump) |
| `citadel_unit_status_width` | devonly, cl | 200 | restart | no |  | Overhead bar width. |
| `citadel_use_vertical_healthbars` | devonly, cl | false | restart | no |  | Vertical orientation for health bars. |
| `pestilence_drone_healthbar_pos` | cl, cheat | 80 | live_cheat | no |  | (no description in dump) |
| `r_citadel_glow_health_bar_debug` | cl, cheat | false | live_cheat | yes: Outlines & glow |  | (no description in dump) |
| `r_citadel_glow_health_bars` | devonly, cl | true | restart | yes: Outlines & glow | D (curated) | Glow health bars visible through walls; already denylisted. |

### Objective health bars (HUD)

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_hud_objective_health_debug_show_midboss` | devonly, cl | false | restart | yes: UI & Panorama |  | (no description in dump) |
| `citadel_hud_objective_health_debug_show_t3` | devonly, cl | 0 | restart | no |  | 0=default 1=friendly 2=enemy 3=both |
| `citadel_hud_objective_health_enabled` | devonly, cl | 2 | restart | yes: UI & Panorama | R | Enable/Disable HUD-level objective health bars.  0=Off. 1=Shrines, Patron, Midboss. 2=T1s and T2s. 3=Barracks |
| `citadel_hud_objective_health_idle_timeout` | devonly, cl | 7 | restart | yes: UI & Panorama | R | After how many seconds of an objective is damage to hide its health on the HUD |
| `citadel_hud_objective_health_lane_max_range` | devonly, cl | 1000 | restart | no | R | (no description in dump) |
| `citadel_hud_objective_health_t1_max_range` | devonly, cl | 1900 | restart | no | R | (no description in dump) |
| `citadel_hud_objective_health_t2_max_range` | devonly, cl | 2500 | restart | no | R | (no description in dump) |

### Damage numbers, damage feedback, DPS meters

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_damage_indicator_enemy_display_time` | devonly, cl | 2 | restart | no | R | (no description in dump) |
| `citadel_damage_indicator_height` | devonly, cl | 120 | restart | no |  | (no description in dump) |
| `citadel_damage_indicator_radius` | devonly, cl | 300 | restart | no | R | (no description in dump) |
| `citadel_damage_indicator_width` | devonly, cl | 120 | restart | no |  | (no description in dump) |
| `citadel_damage_offscreen_indicator_disabled` | cl, release | true | live | yes: UI & Panorama | R | Off-screen damage indicator off by default. |
| `citadel_damage_radar_enemy_display_time` | devonly, cl | 2 | restart | no | R | (no description in dump) |
| `citadel_damage_report_enable` | devonly, cl | true | restart | yes: UI & Panorama |  | Enable damage report panel. |
| `citadel_damage_report_show_adjusted_percent_min` | devonly, cl | 3 | restart | no |  | Hide buffed/resisted damage if it's below this percentage of change |
| `citadel_damage_report_show_always` | devonly, cl, a | false | restart | no |  | Show damage report without holding alt. |
| `citadel_damage_summary_max_entries` | devonly, cl | 8 | restart | no |  | (no description in dump) |
| `citadel_damage_summary_show_time` | devonly, cl | 12 | restart | no |  | (no description in dump) |
| `citadel_damage_text_batching_window_ability` | devonly, cl | 1.05 | restart | yes: World detail |  | When ability damage events are within this amount of time of each other, they will be added together into a single entry. |
| `citadel_damage_text_batching_window_bullet` | devonly, cl | 1.5 | restart | no |  | When bullet damage events are within this amount of time of each other, they will be added together into a single entry. |
| `citadel_damage_text_batching_window_cumulative` | devonly, cl | 1.5 | restart | no |  | When cumulative damage events are within this amount of time of each other, they will be added together into a single entry. |
| `citadel_damage_text_batching_window_pure` | devonly, cl | 1.05 | restart | no |  | When pure damage events are within this amount of time of each other, they will be added together into a single entry. |
| `citadel_damage_text_distance_far` | devonly, cl | 4000 | restart | no |  | Far distances at which we use far offsets for damage numbers |
| `citadel_damage_text_distance_near` | devonly, cl | 100 | restart | no |  | Near distance at which we use the near offsets for damage numbers |
| `citadel_damage_text_height_offset_far` | devonly, cl | 200 | restart | no |  | How much to offset damage numbers above when far from the camera |
| `citadel_damage_text_height_offset_near` | devonly, cl | 130 | restart | no |  | How much to offset damage numbers above when near from the camera |
| `citadel_damage_text_lifetime` | devonly, cl | 1.5 | restart | yes: UI & Panorama |  | Floating damage number lifetime; 0 disables (autoexec preset comment). Already in catalog. |
| `citadel_damage_text_lifetime_new` | devonly, cl | 1.5 | restart | yes: UI & Panorama |  | How long do accumulated numbers live. |
| `citadel_damage_text_new_ability_offset_x` | devonly, cl | -20 | restart | no |  | (no description in dump) |
| `citadel_damage_text_new_ability_offset_y` | devonly, cl | -25 | restart | no |  | (no description in dump) |
| `citadel_damage_text_new_bullet_offset_x` | devonly, cl | 35 | restart | no |  | (no description in dump) |
| `citadel_damage_text_new_bullet_offset_y` | devonly, cl | -25 | restart | no |  | (no description in dump) |
| `citadel_damage_text_new_melee_offset_x` | devonly, cl | 20 | restart | no |  | (no description in dump) |
| `citadel_damage_text_new_melee_offset_y` | devonly, cl | -60 | restart | no |  | (no description in dump) |
| `citadel_damage_text_new_pure_offset_x` | devonly, cl | -20 | restart | no |  | (no description in dump) |
| `citadel_damage_text_new_pure_offset_y` | devonly, cl | -60 | restart | no |  | (no description in dump) |
| `citadel_damage_text_new_x_offset` | devonly, cl | 15 | restart | no |  | How much to offset damage numbers left |
| `citadel_damage_text_show_effectiveness` | devonly, cl | false | restart | yes: UI & Panorama | R | Shows damage effectiveness on each number. |
| `citadel_damage_text_spacing` | devonly, cl | 20 | restart | no |  | Spacing between floating damage numbers. |
| `citadel_damage_text_x_offset_far` | devonly, cl | 0 | restart | no |  | How much to offset damage numbers left and right when far from the camera |
| `citadel_damage_text_x_offset_near` | devonly, cl | 0 | restart | no |  | How much to offset damage numbers left and right when near the camera |
| `citadel_event_indicator_distance_extremely_far` | devonly, cl | 3000 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_distance_far` | devonly, cl | 1000 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_distance_normal` | devonly, cl | 300 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_distance_very_far` | devonly, cl | 2000 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_dps_percent_high` | devonly, cl | 0.5 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_dps_percent_low` | devonly, cl | 0.1 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_dps_percent_mid` | devonly, cl | 0.3 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_mitigation_high` | devonly, cl | 0.6 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_mitigation_low` | devonly, cl | 0.2 | restart | no |  | (no description in dump) |
| `citadel_event_indicator_mitigation_mid` | devonly, cl | 0.5 | restart | no |  | (no description in dump) |
| `citadel_gold_text_height_offset` | devonly, cl | 35 | restart | no |  | How much higher should gold text show up. |
| `citadel_hud_damage_counter_duration` | devonly, cl | 4 | restart | no |  | (no description in dump) |
| `citadel_hud_heal_counter_duration` | devonly, cl | 4 | restart | no |  | (no description in dump) |
| `citadel_item_used_text_height_offset` | devonly, cl | 25 | restart | no |  | How much higher item used text show up. |
| `citadel_modifier_history_show_time` | devonly, cl | 6 | restart | no |  | (no description in dump) |
| `citadel_show_falloff_in_world` | devonly, cl | false | restart | no | R | (no description in dump) |
| `citadel_show_modifier_history` | devonly, cl | false | restart | no | D | Turns on/off debug display of modifiers |
| `citadel_trooper_offscreen_indicator_range` | devonly, cl | 35 | restart | no | R | (no description in dump) |
| `citadel_ui_damage_impact_duration` | devonly, cl, rep | 2 | restart | no |  | (no description in dump) |
| `citadel_ui_damage_impact_duration_fadeindelay` | devonly, cl, rep | 0 | restart | no |  | (no description in dump) |
| `citadel_ui_damage_impact_duration_fadeoutdelay` | devonly, cl, rep | 0.6 | restart | no |  | (no description in dump) |
| `citadel_ui_damage_impact_kill_duration` | devonly, cl, rep | 5 | restart | no |  | (no description in dump) |
| `citadel_ui_damage_impact_min_max_shield_width` | devonly, cl, rep | 0.1 | restart | no |  | (no description in dump) |
| `citadel_ui_damage_impact_show_for_everything` | devonly, cl | false | restart | no |  | (no description in dump) |
| `citadel_ui_damage_impact_status_minimum_duration` | devonly, cl | 0.2 | restart | no |  | (no description in dump) |
| `cl_ent_show_damage` | cl, cheat | - | live_cheat | no | D | Shows damage numbers over target head. |
| `damage_indicator_safe_area` | devonly, cl | 6 | restart | no |  | (no description in dump) |
| `hud_damagemeter` | cl, cheat | false | live_cheat | no | R | DPS meter. |
| `hud_damagemeter_ooctimer` | devonly, cl | 3 | restart | no |  | How many seconds after the last damage event before we consider the player out of combat. |
| `hud_damagemeter_report` | devonly, cl | true | restart | no |  | Display end-of-combat DPS result (from first damage even to last before OOC timer hit). |

### Chat, pings, chat wheel

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `chat_max_messages` | devonly, cl | 50 | restart | no |  | (no description in dump) |
| `chat_ping_repeat_seconds` | devonly, cl | 5 | restart | no |  | (no description in dump) |
| `chat_sequence_max_messages` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `chat_sequence_within_seconds` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `chat_top_bar_max_messages` | devonly, cl | 6 | restart | no |  | The maximum amount of chat panels we allow total |
| `chat_top_bar_max_messages_per_player` | devonly, cl | 3 | restart | no |  | The maximum amount of chat panels we allow per player |
| `citadel_allow_ally_pings` | cl, a, release | true | live | no |  | (no description in dump) |
| `citadel_allow_party_pings` | cl, a, release | true | live | no |  | (no description in dump) |
| `citadel_allow_spectated_pings` | cl, a, release | true | live | no |  | (no description in dump) |
| `citadel_allow_spectated_text` | cl, a, release | true | live | no |  | (no description in dump) |
| `citadel_auto_ping_window` | devonly, cl | 0.35 | restart | yes: World detail |  | If the chat wheel is opened and closed within this time, trigger a ping when it's closed. |
| `citadel_chat_fade_time` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `citadel_chat_fade_time_extension` | devonly, cl | 7 | restart | no |  | (no description in dump) |
| `citadel_clamp_mouse_when_using_ping_wheel` | devonly, cl | true | restart | no |  | (no description in dump) |
| `citadel_dead_zone_radius` | devonly, cl | 0.05 | restart | no |  | (no description in dump) |
| `citadel_dead_zone_radius_instant_wheel` | devonly, cl | 0.1 | restart | no |  | (no description in dump) |
| `citadel_enable_double_ping` | devonly, cl | true | restart | no |  | Turns off the ability to have aggressive and passive pings. |
| `citadel_enable_new_ping_particle` | devonly, cl | false | restart | yes: Particles & effects |  | Convar to test new ping particle |
| `citadel_hud_chat_wheel` | devonly, cl | true | restart | no |  | (no description in dump) |
| `citadel_invert_ping_type` | cl, a | false | live | no |  | Inverts the ping types so single ping would be aggressive and double ping would be passive |
| `citadel_ping_allow_responses_to_yourself` | devonly, cl | false | restart | no |  | Allow you to respond to yourself. |
| `citadel_ping_indicator_display_time` | devonly, cl | 5.5 | restart | no |  | (no description in dump) |
| `citadel_ping_indicator_duration` | devonly, cl | 6 | restart | no |  | The amount of time the in-world ping indicator stays. |
| `citadel_ping_indicator_duration_for_bosses` | devonly, cl | 2 | restart | no |  | The amount of time the in-world ping indicator stays when a boss pings itself. |
| `citadel_ping_wheel_activation_radius` | devonly, cl | 0.37 | restart | yes: UI & Panorama |  | LEGACY. See: citadel_show_chat_wheel_angle_threshold. Increase this to change how much you have to move your mouse to make the mousewheel visible. |
| `citadel_respond_to_ping_time` | devonly, cl | 5 | restart | no |  | The amount of time you have to respond to a ping from another player |
| `citadel_send_text_chat_to_player_pings` | devonly, cl | true | restart | no |  | (no description in dump) |
| `citadel_show_chat_wheel_angle_threshold` | devonly, cl | 16 | restart | yes: UI & Panorama |  | (degrees) Increase this to change how much you have to move your camera angle to make the Chat Wheel instantly visible while holding Ping. |
| `citadel_show_chat_wheel_time` | devonly, cl | 0.23 | restart | yes: UI & Panorama |  | How long it takes after pressing the +ping command for the chat wheel to appear. |
| `citadel_text_chat_enabled` | devonly, sv, cl, rep | true | restart | no |  | (no description in dump) |
| `citadel_use_contextual_ping_wheel_option` | cl, a | true | live | no |  | (no description in dump) |
| `cl_chat_active` | devonly, cl | 0 | restart | no |  | (no description in dump) |
| `cl_hud_telemetry_ping_poor` | cl, a, release | 100 | live | no |  | Ping higher than this (ms) is considered 'poor'. |
| `cl_hud_telemetry_ping_show` | cl, a, release | 1 | live | no |  | Ping readout: 0 never, 1 poor only, 2 always. |
| `deadlock_chat_mode` | cl, a, release | 2 | live | no |  | Default communication preference for players |
| `ping_indicator_safe_area_x` | devonly, cl | 480 | restart | no |  | (no description in dump) |
| `ping_indicator_safe_area_y` | devonly, cl | 200 | restart | no |  | (no description in dump) |
| `ping_quick_response` | cl, release | - | live | no |  | Responds to the last ping message received |
| `ping_target_reset_time` | devonly, cl | 1.2 | restart | no |  | (no description in dump) |
| `ping_trace_radius` | devonly, cl | 5 | restart | no |  | (no description in dump) |
| `ping_trace_radius_expanded` | devonly, cl | 60 | restart | no |  | (no description in dump) |
| `pingkeypress` | cl, release | - | live | no |  | Ping keybind pressed |
| `player_ping_indicator_boss_offset` | devonly, cl | 110 | restart | no |  | (no description in dump) |
| `player_ping_indicator_default_offset` | devonly, cl | 50 | restart | no |  | (no description in dump) |
| `player_ping_indicator_enabled` | devonly, cl | true | restart | no |  | (no description in dump) |
| `player_ping_indicator_local_player` | devonly, cl | false | restart | no |  | (no description in dump) |
| `player_ping_indicator_player_offset` | devonly, cl | 60 | restart | no |  | (no description in dump) |
| `player_ping_indicator_scale_max_distance` | devonly, cl | 4000 | restart | no |  | (no description in dump) |
| `player_ping_indicator_scale_max_scale` | devonly, cl | 1 | restart | no |  | (no description in dump) |
| `player_ping_indicator_scale_min_distance` | devonly, cl | 1300 | restart | no |  | (no description in dump) |
| `player_ping_indicator_scale_min_scale` | devonly, cl | 0.5 | restart | no |  | (no description in dump) |
| `say_chat` | cl, release | - | live | no |  | Opens chat menu to chat with everyone |
| `say_chat_team` | cl, release | - | live | no |  | Opens chat menu to chat with Allies |
| `team_chat_auto_join` | cl, a, release | false | live | no |  | Auto-join Team Chat when joining a match. Will be overridden by any party settings. |
| `team_chat_hold_join_time` | devonly, cl | 1 | restart | no |  | (no description in dump) |
| `tv_chatgroupsize` | release | 0 | live | no |  | Set the default chat group size |
| `tv_chattimelimit` | release | 0.2 | live | no |  | Limits spectators to chat only every n seconds |

### Top bar, announcements, toasts, event timers

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `announce_show_ids` | cl, release | false | live | no |  | When set, will show the IDs of the various announcements, making updating/deleting easier |
| `citadel_announcement_banned_heroes_display_time` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `citadel_announcement_display_time` | devonly, cl | 4 | restart | no |  | (no description in dump) |
| `citadel_announcement_game_over_msg_display_time` | devonly, cl | 11 | restart | no |  | (no description in dump) |
| `citadel_capturepoint_show_event_timer` | devonly, sv, cl, rep | false | restart | no | R | (no description in dump) |
| `citadel_commend_toast_enemy_seconds` | devonly, cl | 4 | restart | yes: UI & Panorama |  | Number of seconds to show enemy commend toasts |
| `citadel_commend_toast_seconds` | devonly, cl | 30 | restart | yes: UI & Panorama |  | Number of seconds to show commend toasts |
| `citadel_event_timer_frequency_imminent` | devonly, cl | 6 | restart | no |  | (no description in dump) |
| `citadel_event_timer_frequency_soon` | devonly, cl | 25 | restart | no |  | (no description in dump) |
| `citadel_event_timer_max_distance_distance` | devonly, cl | 5000 | restart | no | R | (no description in dump) |
| `citadel_event_timer_max_distance_scale` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `citadel_event_timer_max_view_angle` | devonly, cl | 45 | restart | no |  | (no description in dump) |
| `citadel_event_timer_min_distance_distance` | devonly, cl | 1000 | restart | no |  | (no description in dump) |
| `citadel_event_timer_min_distance_scale` | devonly, cl | 1.5 | restart | no |  | (no description in dump) |
| `citadel_event_timer_min_view_angle` | devonly, cl | 12 | restart | no |  | (no description in dump) |
| `citadel_event_timer_scale_on_direct_look` | devonly, cl | 1.33 | restart | no |  | (no description in dump) |
| `citadel_hide_modifier_bars_on_left_hud` | devonly, cl | false | restart | no |  | Hide modifier bars on the left HUD. |
| `citadel_hud_announcement_display_time_all_queued` | devonly, cl | 7 | restart | no |  | (no description in dump) |
| `citadel_hud_announcement_display_time_max` | devonly, cl | 3 | restart | no |  | (no description in dump) |
| `citadel_hud_announcement_force_single_levelup` | devonly, cl | false | restart | no |  | (no description in dump) |
| `citadel_hud_top_bar_enable_dynamic_player_position` | devonly, cl | true | restart | no |  | Top bar player portraits reorder dynamically. |
| `citadel_modifier_hud_message_display_clear` | devonly, cl | false | restart | no |  | (no description in dump) |
| `citadel_modifier_hud_message_display_min_panel_count` | devonly, cl | 4 | restart | no |  | (no description in dump) |
| `citadel_modifier_hud_message_display_time` | devonly, cl | 2.5 | restart | no |  | (no description in dump) |
| `citadel_powerup_spawner_show_event_timer` | devonly, cl, rep | false | restart | no | R | (no description in dump) |
| `citadel_show_all_purchase_toasts` | cl, a | false | live | no |  | Toasts for all purchases, not only quickbuy queue. |
| `citadel_show_new_topbar` | devonly, cl | false | restart | no |  | Switches to the new top bar layout. Only top-bar layout toggle found. |
| `citadel_show_seasonal_kill_toast` | devonly, cl | - | restart | no |  | (no description in dump) |
| `citadel_show_stats_tooltips_on_scoreboard` | devonly, cl | true | restart | no |  | (no description in dump) |
| `cl_screenmessage_notifytime` | devonly, cl | 8 | restart | no |  | How long to display screen message text |
| `cl_showtextmsg` | devonly, cl | true | restart | no |  | Enable/disable text messages printing on the screen. |
| `screenmessage_show` | cheat | -1 | live_cheat | no |  | Enable display of console messages on screen. 1 = Enabled, 0 = Disabled, -1 = Enabled if vgui is not present |
| `toast_manager_override_duration` | devonly, cl | -1 | restart | no |  | (no description in dump) |

### Ability slots, item slots, shop, radial menus

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_always_show_active_hud_stats` | cl, a | false | live | no |  | Keep active stats panel visible. |
| `citadel_hud_build_category_max_height` | devonly, cl | 600 | restart | no |  | (no description in dump) |
| `citadel_hud_build_category_min_height` | devonly, cl | 185 | restart | no |  | (no description in dump) |
| `citadel_hud_build_category_min_width` | devonly, cl | 125 | restart | no |  | (no description in dump) |
| `citadel_in_world_item_panel_dpi` | devonly, cl | 2 | restart | yes: Advanced |  | In-world texture resolution scale |
| `citadel_item_idol_label_offset` | cl, cheat | 50 | live_cheat | no |  | (no description in dump) |
| `citadel_item_neutral_gold_label_offset` | cl, cheat | 6 | live_cheat | no |  | (no description in dump) |
| `citadel_item_rejuvenator_label_offset` | cl, cheat | 160 | live_cheat | no |  | (no description in dump) |
| `citadel_profile_tooltip_enabled` | devonly, cl | true | restart | no |  | (no description in dump) |
| `citadel_radial_ability_suggestion_weight` | devonly, cl | 0 | restart | no |  | How much extra weight to give a segment when it's the next recommended ability. |
| `citadel_radial_distortion` | devonly, cl | 0 | restart | no |  | 0: Off 1: Distorts the visible distribution of arcs based on the mouse pointer. |
| `citadel_radial_distortion_growth_factor` | devonly, cl | 1.25 | restart | no |  | When the cursor enters a radial arc fully, how much should it grow by (in terms of weight) |
| `citadel_radial_testing` | devonly, cl | 0 | restart | no |  | 0: Normal. 1: Inhibit showing the hud abilities' upgrade panel when the scoreboard is open. |
| `citadel_shop_default_tab` | cl, a, release | -1 | live | no |  | Default shop tab. |
| `citadel_shop_items_appear_enhanced` | cl, cheat | false | live_cheat | no |  | Makes all of the items in the shop appear enhanced if they can be enhanced |
| `citadel_shop_reset_time` | devonly, cl | 10 | restart | no |  | (no description in dump) |
| `citadel_show_active_slot_popup` | cl, a, release | false | live | no |  | Popup when an active item slot changes. |
| `citadel_show_movement_speed_in_units` | devonly, cl | false | restart | no |  | Movespeed in units not m/s. |
| `citadel_show_movespeed_on_hud` | devonly, cl | false | restart | no |  | Movespeed readout on HUD. |
| `citadel_show_new_mod_tooltips` | devonly, cl | true | restart | no |  | (no description in dump) |
| `citadel_use_shop_component_groupings` | cl, a | false | live | no |  | Grouped component layout in shop. |
| `cl_citadel_ability_alt_cast_hold_time` | cl, a, user | 0.15 | live | no |  | (no description in dump) |
| `cl_citadel_ability_alt_cast_instant_cast_double_tap_timeout` | cl, a, user | 0.2 | live | no |  | (no description in dump) |
| `cl_citadel_ability_alt_cast_mode` | cl, a, user | 2 | live | no |  | (no description in dump) |
| `cl_citadel_cancel_with_ability_key_enabled` | cl, a, user | false | live | no |  | (no description in dump) |
| `cl_citadel_items_quickcast_mode` | cl, a, user | 0 | live | no |  | (no description in dump) |
| `cl_citadel_quickcast_ability1` | cl, a, user | 0 | live | no |  | (no description in dump) |
| `cl_citadel_quickcast_ability2` | cl, a, user | 0 | live | no |  | (no description in dump) |
| `cl_citadel_quickcast_ability3` | cl, a, user | 0 | live | no |  | (no description in dump) |
| `cl_citadel_quickcast_ability4` | cl, a, user | 0 | live | no |  | (no description in dump) |
| `hud_fastswitch` | cl, a | 0 | live | no |  | (no description in dump) |
| `sticky_tooltips` | devonly, cl | false | restart | no |  | Don't ever hide tooltips. Helpful when debugging complicated tooltip layouts. |

### Hero portraits

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_portrait_unit_ag2_enable` | devonly, cl | true | restart | yes: UI & Panorama |  | Enable AG2 use in portrait units |
| `citadel_portrait_world_renderer_off` | devonly, cl | false | restart | yes: UI & Panorama |  | (no description in dump) |
| `r_citadel_portrait_allow_particle_only` | devonly, cl | true | restart | no |  | (no description in dump) |
| `r_citadel_portrait_highlight_particle_only` | devonly, cl | false | restart | no |  | (no description in dump) |

### Global HUD visibility, cursor, scaling

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_display_new_player_recommendations` | cl, release | true | live | no |  | Do we want to show the decorations for new player friendly heroes? |
| `citadel_enable_survey` | devonly, cl | true | restart | no |  | Kill switch in case we want to make sure the survey isn't shown, or temporarily disable it |
| `citadel_hide_replay_hud` | cl, release | false | live | no |  | (no description in dump) |
| `citadel_hint_system_disable` | cl, release | false | live | no |  | Set to disable hints |
| `citadel_hud_exclusive_visible_id` | cl, cheat | - | live_cheat | no | R | Show only the panel with the given id (hides rest of HUD). |
| `citadel_hud_visible` | cl, release | true | live | no |  | Master HUD on/off (release-safe). |
| `citadel_new_player_flow_visible` | cl, a, release | true | live | no |  | Are we still showing the new player instructions |
| `citadel_show_page_reload_button` | cl, a | true | live | no |  | Show beta db controls in the upper left corner |
| `citadel_time_after_damage_to_show_hints` | cl, release | 10 | live | no |  | Time after the local player has taken damage from another player before we show hints again. |
| `citadel_use_spectator_team_colors` | devonly, cl | false | restart | no | R | Draw team colors as spectator. |
| `citadel_use_ui_keybindings` | cl, a, release | true | live | no |  | Use UI key bindings otherwise use engine keybindings. |
| `citadel_zipline_arrow_scale` | devonly, cl | 1 | restart | no |  | Changes the zipline arrow scale |
| `citadel_zipline_show_enemy_boosting` | sv, cl, rep, cheat | 1 | live_cheat | no | D | 0 = no, 1 = yes, 2 = preview effect |
| `citadel_zipline_show_lane_colors_for_spectators` | devonly, cl | false | restart | no | R | (no description in dump) |
| `cl_auto_cursor_scale` | a | true | live | no |  | Auto cursor scaling with resolution. |
| `cl_cursor_scale` | a | 1 | live | no |  | Mouse cursor size multiplier. |
| `cl_drawhud` | cl, cheat | true | live_cheat | no |  | Hides/shows entire HUD; cheat-gated. |
| `deadlock_disable_post_match_survey` | cl, a | false | live | no |  | Disable the early post match survey |
| `hud_free_cursor` | cl, release | -1 | live | yes: UI & Panorama |  | If -1 use the hud default, otherwise 0 is disabled, 1 is enabled |
| `hud_free_cursor_toggle` | cl, release | - | live | no |  | Toggles free cursor convar. |
| `hud_reloadscheme` | devonly, cl | - | restart | no |  | Reloads hud layout and animation scripts. |
| `mat_viewportscale` | devonly, cl | 1 | restart | yes: Textures & shaders |  | Scale down the main viewport (to reduce GPU impact on CPU profiling) |
| `r_dashboard_render_quality` | devonly, cl | true | restart | yes: UI & Panorama |  | (no description in dump) |
| `r_drawpanorama` | cheat | true | live_cheat | no |  | Disables all Panorama UI rendering (menus too); cheat-gated. |
| `r_drawviewmodel` | cl, cheat | true | live_cheat | yes: Camera & view |  | Hides first-person weapon model. |
| `survey_chance` | cl, release | 0 | live | no |  | Percentage chance of showing the survey questions when entering matchmaking |
| `survey_min_games_played` | cl, release | 75 | live | no |  | Don't allow for showing the survey unless a minimum number of games have been played |
| `ui_hud_dist` | devonly, cl, rep | 24 | restart | no |  | Distance from player to HUD (world-panel HUD). |
| `viewmodel_fov` | cl, cheat | 54 | live_cheat | yes: Camera & view |  | Viewmodel FOV (not HUD proper, same slider group in other tools). |

### Telemetry and FPS overlays

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_show_telemetry_settings` | cl, release | false | live | no |  | Show HUD Telemetry Settings. |
| `cl_hud_telemetry_frametime_poor` | cl, a, release | 100 | live | no |  | Frame time greater than this is considered 'poor'. |
| `cl_hud_telemetry_frametime_show` | cl, a, release | 1 | live | yes: UI & Panorama |  | Frame time readout: 0/1/2. |
| `cl_hud_telemetry_net_detailed` | cl, a, release | 0 | live | no |  | Show breakdown network misdelivery (loss, late delivery, and peak jitter).  0=never, 1=only in poor network conditions, 2=always |
| `cl_hud_telemetry_net_misdelivery_poor` | cl, a, release | 5 | live | no |  | Packet delivery anomaly rate (0..100) higher than this is considered 'poor'. |
| `cl_hud_telemetry_net_misdelivery_show` | cl, a, release | 1 | live | no |  | Show percentage of user commands & server snapshots that are missed due to network conditions.  0=never, 1=only in poor conditions, 2=always |
| `cl_hud_telemetry_net_quality_graph_show` | cl, a, release | 0 | live | no |  | Packet jitter graph. |
| `cl_hud_telemetry_serverrecvmargin_graph_show` | cl, a, release | 0 | live | no |  | Show graph of the server recv margin in the HUD.  (How early/late user commands are arriving at the server before they are executed.)   0=never, 1=onl |
| `cl_showdemooverlay` | devonly | 0 | restart | no |  | How often to flash demo recording/playback overlay (0 - disable overlay, -1 - show always) |
| `cl_showerror` | cl, release | 0 | live | no |  | Show prediction errors, 2 for above plus detailed field deltas, 3 to filter out serverside known prediction errors, -entindex for specific entity. |
| `cl_showfps` | cl, release | 0 | live | no |  | FPS meter at top of screen (1 fps, 2 smooth, 3 server MS, 4 both). |
| `cl_showframenumber` | cl, release | false | live | no |  | Show current framenumber |
| `cl_showmem` | cl, release | 0 | live | no |  | Draw approximate memory use at top of screen |
| `cl_showpos` | cl, cheat, release | 0 | live_cheat | no |  | Draw current position at top of screen |
| `cl_showtick` | cl, release | 0 | live | no |  | Show current tick/time values.  Bitmask:  1='render time'  2='GameTime'   4=time of predicted entities  8=offset of predicted entities    (-1 means 'e |

### Captions and subtitles

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `cc_captiontrace` | devonly, cl | 1 | restart | yes: UI & Panorama |  | Show missing closecaptions (0 = no, 1 = devconsole, 2 = show in hud) |
| `cc_spectator_only` | cl, a | false | live | no |  | (no description in dump) |
| `cc_subtitles` | cl, a | false | live | no |  | Voice-over captions only. |
| `cc_vr_font_size` | cl, a | 1 | live | no |  | 0 = small, 1 = med (default), 2 = large |
| `closecaption` | cl, a, user | false | live | yes: UI & Panorama |  | Enable closed captions. |

### Spectator HUD

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `citadel_spectate_directed_mode_enabled` | devonly, sv, cl, rep | false | restart | no |  | (no description in dump) |
| `citadel_spectator_mode` | devonly, cl | 0 | restart | no | R | Toggles the spectator mode: 0=Directed - 1=Free Cam - 2=Hero Chase - 3=PlayerView |
| `citadel_spectator_voice_mode` | cl, user | true | live | no |  | Spectator voice transmit mode: 0 spectators and players, 1 spectators only |
| `citadel_spectator_voice_mode_toggle` | cl, release | - | live | no |  | Toggle the value of citadel_spectator_voice_mode |

### Panorama engine (UI renderer)

10 user-relevant entries listed; 100 further `panorama_*` entries are renderer/debug internals (all devonly, restart class) and omitted.

| Name | Flags | Default | Apply | In catalog.toml | Deny | Description |
|---|---|---|---|---|---|---|
| `panorama_allow_transitions` | devonly | true | restart | yes: UI & Panorama |  | UI transitions on/off. |
| `panorama_disable_blur` | devonly | false | restart | yes: UI & Panorama |  | Disable blur in UI. |
| `panorama_disable_box_shadow` | devonly | false | restart | yes: UI & Panorama |  | Disable box shadows. |
| `panorama_disable_draw_text_shadow` | devonly | false | restart | no |  | Disable text shadows. |
| `panorama_disallow_hover_styles` | devonly | false | restart | no |  | Disable hover styles. |
| `panorama_max_fps` | devonly | 120 | restart | yes: UI & Panorama |  | Frame cap for Panorama UI. |
| `panorama_max_overlay_fps` | devonly | 60 | restart | yes: UI & Panorama |  | FPS cap for overlay panels. |
| `panorama_panel_occlusion` | devonly | true | restart | yes: UI & Panorama |  | Skip occluded panels. |
| `panorama_show_fps` | devonly | false | restart | no |  | Panorama FPS counter. |
| `panorama_transition_time_factor` | devonly | 1 | restart | yes: UI & Panorama |  | Scale for UI transition speed. |