# GUI assets

| File | What it is | Origin and licence |
|---|---|---|
| `fonts/Inter-*.ttf` | UI font, subset by `scripts/subset-fonts.sh` | Inter, SIL Open Font License (`fonts/OFL.txt`) |
| `fonts/Hack-Regular.ttf` | Monospace font, subset by the same script | Hack, MIT and Bitstream Vera licence (`fonts/Hack-LICENSE.md`) |
| `vanilla_hud.jpg` | The HUD layout preview's backdrop and the "In game" crops on the HUD pages | Simon Khalimonov's own Deadlock screenshot at default HUD settings (2048x1152, 2026-10-05), shipped with his permission. The net graph (top right) and the match id and server CPU text (bottom right) are blurred out, then the image is scaled to 1280x720 and saved as JPEG quality 70 |

Element positions in `vanilla_hud.jpg` are the `Measured::Screenshot` rows of `dt_core::hud::elements::ELEMENTS`, pinned by `vanilla_boxes_sit_on_the_reference_screenshot`. A new screenshot means re-measuring those rows.
