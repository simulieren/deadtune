# Panorama at runtime (retail Deadlock, build 25712201)

Source: Simon's console on Windows, 2026-10-06: `find reload`, `find panorama`, `dump_panorama_css_properties`, `dump_panorama_events`. Summary, not the raw dumps.

## No reload

`find reload` and `find panorama` list no Panorama reload command. The `script_reload*` / `cl_script_reload*` commands are cheat-flagged and reload VScript, not Panorama. `r_drawpanorama` is cheat. `panorama_debugger_theme` (client, archive) shows the Panorama debugger ships, but its open command is not listed (likely dev-only, hidden from `find`). Conclusion: anything shipped in DeadTune's HUD pak needs a game restart to change.

## Style properties a script can set live (`panel.style.<prop>`)

Every DeadTune HUD edit maps to a property the engine documents:

| DeadTune edit | Property |
|---|---|
| Move | `position`, `x`, `y`, `transform: translate3d(...)`, `margin-*` |
| Resize | `ui-scale` (layout-level, text re-rasterised), `pre-transform-scale2d` (bitmap), `width`/`height` |
| Fade / hide | `opacity`, `opacity-brush`, `visibility: collapse` |
| Tint | `wash-color` (alpha = strength) |
| Hue / saturation / brightness / contrast | `hue-rotation`, `saturation` (0 = greyscale), `brightness`, `contrast` (composition-time, whole subtree) |
| Swap an image | `background-image: url("file://{images}/...")` |
| Blur | `blur`, `background-blur`, `world-blur` |
| Text | `color`, `font-size`, `font-weight`, `text-shadow`, `letter-spacing` |
| Outline / glow | `box-shadow`, `img-shadow`, `border*` |
| Smooth change | `transition*` |

So the colour edits DeadTune bakes into images (tint, hue, saturation, brightness, contrast) also exist as live composition filters; greyscale for dead heroes is `saturation: 0`.

## Events

Only generic panel events: `AddStyle`, `RemoveStyle`, `ToggleStyle`, `SwitchStyle(slot, class)`, `TriggerStyle`, `AddTimedStyle`, `AsyncEvent(delay, event)`, `IfHasClassEvent`, plus Citadel tooltip events. No settings-changed or ConVar-changed event, so a live script polls (the Wide FOV Slider mod polls every 0.25 s). `SwitchStyle` lets precompiled CSS classes act as live presets.

## Data channel (proven 2026-10-06)

The game's `CitadelSettingsSlider` writes its bound ConVar live, dev-only ConVars included (DeadTune's Wide FOV row works in game). A hidden slider bound to a harmless ConVar is therefore a readable value for a script; DeadTune sets that ConVar through the console bridge and the script restyles the HUD on its next poll.
