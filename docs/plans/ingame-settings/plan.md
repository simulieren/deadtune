# In-game settings rows (live dev-only ConVars)

Status: research done, nothing built. Last update 2026-10-05.

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
- The mod's layouts are compiled with a newer KV3 version than `hud::kv3` reads: `hud::inject::tree` fails with `Kv3(Magic(55987030))`. Strings can still be read out of the LaCo block.

## Simon's test (2026-10-05): the row does not show up

Likely causes, in order:

1. **Not mounted.** The zip contains `widefovslider_dir.vpk`. The game only mounts addons named `pakNN_dir.vpk` in `game/citadel/addons`; the mod's README says to rename it (e.g. `pak50_dir.vpk`) or install it through a mod manager.
2. **Stale settings layout.** It replaces the whole 536 KB settings menu as it was when built. If a later patch changed `popup_settings`, the game may reject or ignore the old file, or another addon that replaces the same file wins (DeadTune's conflict scan reports those).
3. **SearchPaths.** Addons need `Game citadel/addons` plus the Mod/Write pins (DeadTune writes them when one of its own addons is installed; with no DeadTune addon installed, a hand-installed mod may not be mounted).

Retest steps: rename to `pak50_dir.vpk`, put it in `game\citadel\addons`, make sure DeadTune's System check shows the addons search path as OK, launch, open Settings > Game > Camera Settings. If the row appears, drag it in a match and note whether the view widens live.

## Plan

### Phase 0: prove the loophole (Windows, 10 minutes)

- [ ] Retest the mod as above. Record: does the row show; does dragging change the view live; does the value survive a restart (via its boot trick).
- [ ] With the mod working, run `r_aspectratio` in the console (no value) and note what it prints after dragging.
- [ ] Decide: if the settings control does not write dev-only ConVars live, stop here; Wide view via `gameinfo.gi` (already in DeadTune) stays the only route.

### Phase 1: read the settings layout

- [ ] Extend `hud::kv3` to the KV3 version the current game uses for `popup_settings.vxml_c` (the magic the decoder rejected), with fixtures from the game snapshot (see `docs/plans/game-files/plan.md`).
- [ ] Decode the game's own `popup_settings.vxml_c` and confirm the Camera Settings anchor (`#citadel_settings_camera_fov` row) in the current build.

### Phase 2: a DeadTune row, rebuilt from the game's file

- [ ] Use `hud::inject` to add one `CitadelSettingsSlider` row bound to `r_aspectratio` right after the stock FOV row, in the game's own current `popup_settings.vxml_c` (never replace it from a stale copy). Rebuild on every game update like the other HUD files.
- [ ] Persistence without the invisible-menu trick: DeadTune already writes `r_aspectratio` to `gameinfo.gi` (Wide view). Our script saves in-game changes to an archived ConVar the game keeps; on its next run DeadTune reads that value from the user config and writes it into `gameinfo.gi`, and the Overview's Wide view slider shows it.
- [ ] Credit: "Idea by Mixboat (Wide FOV Slider); rebuilt by DeadTune from your game files."
- [ ] Launch guard covers it; System check lists other addons replacing `popup_settings`.

### Phase 3: a DeadTune section (only if Phase 0 shows live writes work)

- [ ] A "DeadTune" group in Settings > Advanced with live sliders for dev-only performance ConVars that today need a restart.
- [ ] Test whether settings-control writes reach any `gameinfo_cannot_override` ConVar at runtime (unknown; probably not).

## Risks

- The settings layout is the largest and most-changed layout in the game; rebuilding from the game's own file each update is mandatory.
- If Valve restricts which ConVars settings controls may write, Phases 2 and 3 stop working; Wide view via `gameinfo.gi` remains.
- Hidden rows and invisible menus are fragile; we avoid them.
