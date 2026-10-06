# In-game settings rows (live dev-only ConVars)

Status: Phases 2 and 3 built (2026-10-05). Phase 0 confirmed by Simon on Windows (2026-10-06): DeadTune's Wide FOV row shows under Camera Settings and dragging it changes the view live, so the game's settings control does write a dev-only ConVar at runtime. The other G-checks (DeadTune group, persistence) are still open. Last update 2026-10-06.

## Goal

Add DeadTune rows to Deadlock's own settings menu (Settings > Game > Camera Settings, and possibly a DeadTune section) so players can change dev-only ConVars live, without restarting. First target: a "Wide FOV" row for `r_aspectratio`.

## What we learned from "Wide FOV Slider" (GameBanana 724244, Mixboat, CC BY-NC-ND 4.0)

Idea only; never ship its files. Extracted copy and decoded scripts: `scratchpad/widefov/` (not in the repo).

The mod ships four files:

| File | What it does |
|---|---|
| `panorama/layout/popups/popup_settings.vxml_c` (536 KB) | The whole settings menu, replaced. Adds `<CitadelSettingsSlider id="AspectRatioFOV" class="VideoPreview" convar="r_aspectratio" min="0.010" max="3.200" snap="0.005">` in a `PopupSettingsSettingsRow` under Camera Settings, plus two hidden sliders (zero height, zero opacity, no hit test): `WideFovStash` bound to `citadel_ability_preview_path_debug_draw_dt` (0 to 20) and `WideFovBootFlag` bound to `r_colorblindsim_fullscreen_cycletime` (0 to 10). Includes `panorama/scripts/widefov.vjs_c`. |
| `panorama/layout/base_dashboard.vxml_c` | The main menu root, replaced, to include `panorama/scripts/widefov_boot.vjs_c`. |
| `panorama/scripts/widefov.vjs_c` | Polls the visible slider every 0.25 s. On change: `citadel_ability_preview_path_debug_draw_dt <10 + value>` then `host_writeconfig`, so the value survives in the user config. Restores the value at boot (see below). Moves the slider's default marker to 2.15. |
| `panorama/scripts/widefov_boot.vjs_c` | About 1 s after the main menu loads: sets `r_colorblindsim_fullscreen_cycletime 7.777`, dispatches `CitadelSettings` to open the settings menu invisibly, retries for up to 30 s. The settings script sees the flag, reads the stash, moves the slider (which writes `r_aspectratio`), and closes the menu with `UIPopupButtonClicked`. |

Key mechanism: `CitadelSettingsSlider` is the game's own settings control, and it writes whatever ConVar its `convar` attribute names. The mod relies on it writing `r_aspectratio`, which is dev-only (the console refuses it; the mod's own comment says the console route "works only if the console allows it").

Other facts:

- `citadel_camera_hero_fov` is clamped to 75 to 90 by the engine; the stock FOV slider is `min="75" max="90" snap="1"`.
- The mod's layouts are compiled in the legacy binary KV3 format (`VKV\x03`, inline values), older than the version 4 and 5 lanes `hud::kv3` read; `hud::inject::tree` failed with `Kv3(Magic(55987030))`. `hud::kv3` reads that format since 2026-10-05, and a game file snapshot decodes the mod's `popup_settings.vxml_c` to a 138 KB XML with the `AspectRatioFOV` row in it (`docs/plans/game-files/plan.md`).

## Simon's test (2026-10-05): the row does not show up

Likely causes, in order:

1. **Not mounted.** The zip contains `widefovslider_dir.vpk`. The game only mounts addons named `pakNN_dir.vpk` in `game/citadel/addons`; the mod's README says to rename it (e.g. `pak50_dir.vpk`) or install it through a mod manager.
2. **Stale settings layout.** It replaces the whole 536 KB settings menu as it was when built. If a later patch changed `popup_settings`, the game may reject or ignore the old file, or another addon that replaces the same file wins (DeadTune's conflict scan reports those).
3. **SearchPaths.** Addons need `Game citadel/addons` plus the Mod/Write pins (DeadTune writes them when one of its own addons is installed; with no DeadTune addon installed, a hand-installed mod may not be mounted).

Retest steps: rename to `pak50_dir.vpk`, put it in `game\citadel\addons`, make sure DeadTune's System check shows the addons search path as OK, launch, open Settings > Game > Camera Settings. If the row appears, drag it in a match and note whether the view widens live.

## Plan

### Phase 0: prove the loophole (Windows, 10 minutes)

- [x] Row shows and dragging changes the view live (DeadTune's own rebuilt row, v0.11.0 or later, 2026-10-06). Restart persistence not yet reported.
- [ ] With the mod working, run `r_aspectratio` in the console (no value) and note what it prints after dragging.
- [x] Decided: live writes work, so Phases 2 and 3 stay. This is also the channel a live HUD preview would use (a DeadTune script reading settings and restyling the HUD at runtime).

### Phase 1: read the settings layout

- [x] `hud::kv3` reads the legacy format the mod's `popup_settings.vxml_c` uses (checked against the mod's file outside the repo, `DEADTUNE_KV3_SAMPLES`). The game's own layouts are version 5, which it already read.
- [x] Game file snapshot taken on Windows (build 25712201, 2026-10-05): all 434 layouts and 439 stylesheets decoded to text, including `popup_settings.xml`. The repo is public and these are Valve's files, so keep the snapshot local and point tests at it through an environment variable (like `DEADTUNE_KV3_SAMPLES`) rather than committing it as a fixture.
- [x] Anchor confirmed in build 25712201: `popup_settings.xml` line 1165-1166, a `PopupSettingsSettingsRow` holding `<CitadelSettingsSlider id="CameraFOV" class="VideoPreview" convar="citadel_camera_hero_fov" min="75" max="90" snap="1">`. Insert our row right after that row.

### Phase 2: a DeadTune row, rebuilt from the game's file

- [x] `hud::ingame` adds a `PopupSettingsSettingsRow` with a `CitadelSettingsSlider` bound to `r_aspectratio` (0.010 to 3.200, snap 0.005, class `VideoPreview`, plain label "Wide FOV": labels starting with `#` need a localization token, plain text shows as is) right after the row holding `CameraFOV`, through a new `inject::Anchor::AfterParentOf` (the stock row has no id). `HudLayout.ingame` (serde default, skipped when vanilla) goes through the normal HUD pipeline, so the menu is rebuilt from the game's own file after every update. Proven against the build 25712201 snapshot (`DEADTUNE_GAME_SNAPSHOT`, test `ingame_rows_rebuild_the_real_settings_layout`): 1482 stock lines unchanged and in order, 15 lines added, `addons verify` accepts the pak.
- [x] Persistence: `assets/ingame_settings.js` polls the slider and writes `citadel_ability_preview_path_debug_draw_dt` (archived `cl, a`, a debug-draw timestep the game never reads in play, and the one stash the mod has used in the field) as 10 plus the ratio, then `host_writeconfig`. `ingame::sync_wide_fov` reads `cfg/user_convars_*.vcfg` (`usercfg`) when DeadTune starts, writes `r_aspectratio` into `gameinfo.gi` losslessly with a backup, records the stash value so each value syncs once (a later DeadTune edit is never undone), and the GUI sets the profile's Wide view to match.
- [x] Credit on the page: "Idea by Mixboat (Wide FOV Slider); rebuilt by DeadTune from your game files."
- [x] System check (and `hud status`) warns about other addons that ship `popup_settings.vxml_c`. The launch guard's FATAL watch only runs during a performance-addon trial; the HUD pak (and so these rows) is outside it, as it is for the top bar and minimap layouts. Covering the HUD pak with the guard is a separate change.

### Phase 3: a DeadTune section (only if Phase 0 shows live writes work)

- [x] Built ahead of Phase 0 on Simon's call: a `citadel_settings_deadtune` subsection appended to Settings > Advanced with 8 rows from the catalog (restart-class, dev-only, not cheat, not denylisted, not `gameinfo_ignored`, high or medium impact; sliders take min, max and snap from the catalog's range and step): shadow quality, ambient occlusion, fog quality, grass, prop draw distance, particle cap, small clutter props, bloom. The GUI's In-game settings page picks which rows appear. The subsection title comes from a localization token the game does not have, so the script sets it; if that fails the header reads `citadel_settings_deadtune` (check G-4).
- [ ] Test whether settings-control writes reach any `gameinfo_cannot_override` ConVar at runtime (unknown; probably not). None of the 8 rows is one.
- [ ] Phase 0 and G-1 to G-7 on Windows. If the control does not write dev-only ConVars live, both phases stay off by default and Wide view via `gameinfo.gi` remains the route.

## Risks

- The settings layout is the largest and most-changed layout in the game; rebuilding from the game's own file each update is mandatory.
- If Valve restricts which ConVars settings controls may write, Phases 2 and 3 stop working; Wide view via `gameinfo.gi` remains.
- Hidden rows and invisible menus are fragile; we avoid them.
