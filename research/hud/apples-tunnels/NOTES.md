# Apples & tunnels on the minimap

Source idea: GameBanana mod 724238 "Apple snacks and Tunnel Map" by FesamAyt, v0.3.1,
CC BY-NC-ND 4.0. DeadTune takes the idea and the factual map positions only; none of the
mod's files ship. Credit line in the app: "Idea by FesamAyt; rebuilt by DeadTune from your
game files". Built: `crates/dt-core/src/hud/apples_tunnels.rs`, script
`crates/dt-core/src/hud/assets/apples_tunnels.js`, GUI card `crates/dt-gui/src/apples_view.rs`.

## What the mod does (decoded v0.3.1)

- Replaces `panorama/layout/hud_minimap.vxml_c` with the vanilla layout as text plus one
  stylesheet and one script include (the same plaintext form `hud::inject` writes).
- 36 green dots on the fixed apple spawn spots on dl_midtown, for every hero, visible in
  tunnel view, not tracking whether an apple is up. 24 purple dots on rat tunnel entrances
  for Rem, Mo & Krill, Rat King and Calico (any form), shown within 0.11 of the map width
  of the local hero, hidden past 0.13, hidden while `in_tunnels`.
- Local hero: the top bar's `.LocalPlayer` portrait, `#PlayerNameNWContainer .HeroName`
  label, matched against English names and `$.Localize` of `#hero_familiar`, `#hero_krill`,
  `#hero_ratking`, `#hero_nano`; two equal readings before showing.
- Position: the `.map_button.player.friend.localplayer` marker's offsets summed up to the
  minimap root, centred, divided by `#MinimapBackgroundTest`'s size. Dots are panels
  created inside `#MinimapBackgroundTest` with `position: u% v% 0`.
- Clear switching: vanilla `hud_minimap.css` hides `.backgroundImage1` and shows
  `.backgroundImage3` under `.dl_midtown.in_tunnels`; the mod keeps the surface at 0.15,
  brightens the tunnel layer by 1.15 and fades over 0.18 s.

## What DeadTune does differently

- Includes our own files into the game's current layout at apply time instead of
  replacing it, so a game update to the minimap layout is picked up on the next apply.
- The positions live in a typed table in dt-core and reach the script as a generated
  `var DT_MAP = {...}` header, so the script holds no map data.
- Entrance visibility is a class (`DtNear`) styled by our stylesheet, not inline style.
- Clear switching on its own is CSS appended to the game's `hud_minimap.vcss_c` and
  rebuilds no layout.

## Refreshing the positions

The tables are valid for Deadlock build 25712201, dl_midtown build 6722. When Valve moves
apples or entrances, either rerun `extract_points.py` on a newer decoded mod script or
measure the spots in the hideout, then update `APPLES`, `TUNNEL_ENTRANCES`, `GAME_BUILD`
and `MAP_BUILD`. `node research/hud/apples-tunnels/simulate.js` exercises the script
against a mock Panorama tree (hero gating, radius hysteresis, tunnel view).

## Unknowns for Windows

- Whether the minimap layout's five textures need entries in the plaintext layout's image
  table (we write an empty table, as for the top bar). The mod's table lists them.
- Whether `is_underground` (mid tunnels) should also hide entrance dots; the mod ignores it.
