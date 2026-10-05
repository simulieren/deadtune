# HUD previews from the game's images

Status: the Minimap (with apples and tunnels), Top bar, Health bar and HUD layout previews draw the game's own pictures when they can read them (`dt_core::hud::art` names the paths, `crates/dt-gui/src/hud_art.rs` decodes them) and hand-drawn shapes otherwise. This page records how each picture and size was chosen, so the next change to the table starts from evidence rather than from the previews' look.

## Sources

| Source | Where | Used for |
|---|---|---|
| Image export of build 25712201 | Simon's "Save all images" folder (`manifest.json` plus `panorama/images/**`, 2705 of 2706 images); on the Mac via `DEADTUNE_PREVIEW_IMAGES=<folder>`, and on the UI images page via `DEADTUNE_IMAGES_FROM=<folder>` | What every candidate picture looks like |
| Decoded layouts and styles of the same build | a `snapshot take` folder's `text/panorama/layout/*.xml` and `text/panorama/styles/*.css` | Which picture each panel draws, its CSS size, wash colours |
| In-game screenshot, default HUD, 16:9 | `crates/dt-gui/assets/vanilla_hud.jpg` (shipped, 1280x720; measured on the 2048x1152 original) | On-screen sizes where the game sizes panels in code, composition, colours |

Sizes below are 1080p px. The stylesheets ship minified; a rule is quoted as `selector { property }`.

## 1. Hero portraits

**Top bar: the `_card` art.** `citadel_hud_top_bar_player.xml` puts a `CitadelHeroBadge id="HeroBadge" class="TopBar"` in `#HeroImageArea`; `hero_badge.xml` gives the badge a `CitadelHeroImage id="HeroImage" scaling="contain"` inside `.HeroImageContainer`, and `hero_badge.css` has `CitadelHeroBadge { width: 88px; height: 120px }`, `.HeroImageContainer { width: 80% }`, `.HeroImageBackground { border-radius: 50%; width: 100%; height: width-percentage(100%); vertical-align: bottom }` and `#HeroImage { opacity-mask: url(hero_badges/hero_image_mask_psd) }`. The mask is 280x380, straight sides and top, a round bottom; of the hero variants in the export only `<hero>_card_psd` is 280x380 (`_sm` 128x128, `_vertical` 120x200, `_mm` 50x50). Compositing `ratking_card_psd` through the mask at 70 px wide over a 70 px disc reproduces the screenshot's Rat King portrait exactly (ears and branches above the disc, the card's painted ground inside it); `_sm` fits inside the disc and `_vertical` is a different crop. So the badge is 70x95 (80% of 88 wide, contain), its disc the bottom 70 px, and the top 25 px of the card stick up above the disc in a straight-sided column.

**Minimap: the `_mm` art.** `hud_minimap.xml`'s `MinimapPlayerSnippet` is `Image id="MainImage"` (set in code) over `Panel id="BackgroundImage"`; `hud_minimap.css`: `.player #MainImage { width: 85%; height: 85% }`, `.player #BackgroundImage { background-color: none; opacity: 0.9; border-radius: 50px }`, `.friend.player #BackgroundImage { background-color: offWhite }`, `.enemy.player #BackgroundImage { background-color: colorEnemy }` (`#FF410D`), `.player.localplayer #MainImage { width: 110% }`, `.player.localplayer #LocalSpecularImage { width: 110%; background-image: url(minimap/player_marker_self_spec_highlight_psd) }`. The disc is a colour, not a picture (`hero_flag_*_mm` are not referenced by the minimap rules). `player_marker_self_psd` (the eye) is only precached. The screenshot's allied icon is a 25 px cream disc with a dark bust in it; the `_mm` busts match. The screenshot's local hero is a 28 px disc with the view cone (`MinimapPlayerFoVConeSnippet`, `player_cone_psd`, `.client_cone_fov.player { width: 65%; height: 65% }`) pointing the way the player looks.

## 2. Objectives

`hud_minimap.css`: `.map_button .boss_health, .map_button .boss_health_bg { background-size: 100% 100%; width: 100%; height: 100% }`, both `.boss_image`, so both washed: `.map_button.enemy.boss .boss_image { wash-color: ColorEnemyObjective }` (`#DC4C2FFF`), `.map_button.friend.boss .boss_image { wash-color: offWhite }`, then per lane `.friend.boss.yellowLane { wash-color: #FFDF40 }`, `.greenLane { magenta }`, `.blueLane { #2EC7E6 }`, `.purpleLane { #6BB247 }` (the class names and colours no longer agree; the screenshot's friendly guardian is green on the green lane).

| Panel | CSS size | Backdrop (`.boss_health_bg`) | Fill (`.boss_health`) | Look |
|---|---|---|---|---|
| `.boss_icon_t1` (guardians, walkers) | 40 | `boss_health_psd` (grey diamond) | `boss_health_fill_psd` (white diamond) | solid diamond; a damaged one shows the dark backdrop at the top |
| `.boss_buildingzip` (the two shrines beside a patron) | 32 | `boss_health_t2_psd` | `boss_health_fill_t2_psd` (white diamond with a hole) | diamond with a dark centre |
| `.boss_icon_t3` (patron) | 80 | `boss_health_final_psd` (arch) | `boss_health_final_fill_psd` | the arch over the base, `transform: rotateZ(180deg)` for `.friend` |
| `.boss_icon_t3 .boss_health_s2` | 80 | `boss_health_final_stage2_psd` (small diamond) | `boss_health_final_stage2_fill_psd` | the diamond inside the arch |

`npc_tower_*`, `npc_marker_*` and `boss_marker_*` are not used by any minimap rule (precached only, or scoreboard art). The screenshot agrees: the enemy base is a red arch with a diamond in its opening, two dark-centred diamonds beside it, a solid diamond on each lane; our base is the same in cream with the arch upside down.

## 3. Marker sizes and the map

The game sizes every `.map_button` in code (`.map_button.player { width: 1% }` is a placeholder, and the HUD's minimap icon ConVars scale them), so sizes come from the screenshot, measured on the 2048 px original and scaled to 1080p against the 360 px map:

| Marker | Measured | Table size |
|---|---|---|
| guardian or walker diamond | 18 px across (the diamond fills 70% of its image) | 26 |
| shrine diamond | 13.5 px | 21 |
| patron arch | 64 px wide (85% of its image) | 75 |
| allied hero disc | 25 px | 25 |
| local hero disc | 28 px, cone about 20 px below it | 30 |
| lane lines | 2 px | |

The previous table drew objectives at 64 px, hence the oversized walkers.

**Map and frame.** `hud_minimap.xml` draws `minimap/base/minimap_midtown_mid_psd` as `.backgroundImage1` (`#hud_minimap { width: 360px; height: 360px }`; `_tunnels` and `_rat_tunnels` are the underground layers at opacity 0). The lanes are on `UICanvas id="canvas"`, painted by the game: no lane image exists, so the preview paints them. `hud.css`: `#minimap_frame { width: 400px; height: 400px; background-image: url(minimap/base/minimap_frame_psd); background-image: url(minimap/compass_frame_psd); opacity-mask: url(minimap/compass_frame_mask_psd); world-blur: ingameHudBlur }`: the compass frame (a dark disc with a lobe top and bottom, where the arches sit) over a blurred world. The preview draws the compass frame over a dark fill. `#minimap_persp { width: 440px; height: 520px }` is the perspective room; `hud::elements` measures the visible map at 370x392.

**Positions.** Calibrated against the screenshot, own base at the bottom: three lanes (green left, blue middle, yellow right on our half; the enemy half of every lane is red), guardians near the mid, shrines at the base lobes, the patron arch in the lobe. The screenshot's hero and camp placements are one match; the preview's are examples.

## 4. Tints

`citadel_base_styles.css`: `team1Color: #D4860B` (amber), `team2Color: #4D75C3` (sapphire), `team1Foreground: #201500`, `team2Foreground: #FFFFFF`, `offWhite: #FFEFD7`, `offBlack: #10130D`, `colorEnemy: #FF410D`, `shardColor: #99FFD6`. `hud_minimap.css` defines `ColorEnemyObjective: #DC4C2FFF`, `ColorTeam1: #E7B659`, `ColorTeam2: #5B79E6`. Minimap heroes and objectives are coloured friend or enemy (offWhite or the enemy colour), not by team; the top bar is coloured by team (`.team1`, `.team2`), and the player's team sits on the left (`#TeamFriendly` before `#TeamEnemy` in `citadel_hud_top_bar.xml`). In the screenshot the player is sapphire, so the preview's allies are sapphire on the left and the enemies amber on the right.

Wash is a multiply: a grey backdrop washed red is dark red, a white fill washed red is bright red. The previews tint the same way (`Images::paint` multiplies the texture by the tint).

## 5. Top bar and health bar geometry

Top bar (`citadel_hud_top_bar.css`, per 88 px player panel, `#TeamsContainer { width: 1260px }`, six panels a side, `.TeamNetworth { width: 300px }` between):

- Badge disc 70 px, bottom at y=100 (`#HeroImageArea { height: 100px }`, `#HeroBadge { vertical-align: bottom }`). Dead: `.Dead` greys the card, `.RespawnTimer { color: #FE420E; font-size: 32px; font-weight: bold }` over it.
- Souls tag: `.SoulsValueContainer { margin-top: 76px; padding: 1px 5px 0 5px; padding-top: 20px; border-radius: 0 0 3px 3px; background-color: team colour }`, `.SoulsValue { font-size: 15px; font-weight: bold }`, `.team1 .SoulsValue { wash-color: team1Foreground }`, `.Dead .SoulsValueContainer { opacity: 0.3 }`: a 34x44 column behind the disc's bottom, 24 px of it showing under the disc.
- Health: `#HealthBar { width: 16px; height: 50px; vertical-align: middle; margin-bottom: 14px }`, `.friend #HealthBar { margin-left: 10px }`, `.enemy #HealthBar { horizontal-align: right; margin-right: 4px }`, border `healthbar_backer_vert_border` washed offBlack, contents `#000000de` through `healthbar_backer_vert_mask`, fill offWhite (`.enemy` `#FF5656`). Only `.HealthVisible` shows it.
- Status row: `#StatusRow { margin-top: 125px; height: 40px }`, `.UltimateStatus { width: 22px; height: 22px; border-radius: 50%; opacity: 0.6 }` on a team-coloured `.UltimateStatusBG`, `#UltimateAbilityIconMini` `icon_ultimate_off` (`icon_ultimate`, the eye, when `.UltimateCooldownReady`).
- Level: `.HeroLevelBacker { width: 20px; height: 20px; margin-bottom: 28px; border: 1.5px solid offWhite; border-radius: 50px; opacity: 0.5 }`, `.HeroLevelLabel { font-size: 12px; font-weight: bold }`.
- Clock: `.GameClock { padding: 5px; margin-top: 8px; background-color: #00000090; border-radius: 5px }`, `.GameTime { width: 80px; height: 20px; font-size: 14px; color: offWhite&90 }`.
- Team souls: `.TeamNetworth { margin-top: 45px }`, `.TeamScore { width: 145px }`, `.ScoreBG` washed in the team colour through `masks/dust_01_psd` with a 50 px slant (`padding-left: 50px`), `.ScoreLabel { font-size: 22px; font-weight: bold; color: white }`, `.TeamIcon { width: 30px; height: 26px }` (`icon_team1_psd` the trident, `icon_team2_psd` the key).

Health bar (`hud_health.css`, `hud_health_container.css`): `#health_bar { width: 66px; height: 212px; background-color: #333333ea; opacity-mask: url(healthbar_backer_mask) }` (a slanted ruler, wider at the top), `.ProgressBarLeft { background-color: #FFEFD7; background-image: url(healthbar_fill_texture_png) }`, `#healthLines .line_large { height: 3px; background-color: offBlack; opacity: 0.7 }` and `.line_small { width: 30% }` (the ticks), `#health_bar_frame { width: 68px; height: 220px; wash-color: #142304; background-image: url(healthbar_frame_with_regen) }` (`.healthLow` `#cc340a`), `.healthBacker { width: 90%; wash-color: vivaciousGreen; background-image: url(hud/core/health_backer); background-position: right bottom }` behind `.currentHealthLabel { font-size: 32px; font-weight: bold; color: offWhite }` (36 px `#FF5656` when low) with `.totalHealthLabel { font-size: 14px; opacity: 0.2; transform: rotateZ(-3deg) }` under it, and `.regen_container` at the top left with the 7x8 arrows at opacity 0.3. The whole block runs at `ui-scale: 120%`.

## Checks

- `dt_core::hud::art` tests: every path is a replaceable image; with `DEADTUNE_GAME_SNAPSHOT=<snapshot>` every path is in that build's `pak01.tsv`; with `DEADTUNE_PREVIEW_IMAGES=<export>` every path is in the export's `manifest.json`. Both ran green on build 25712201.
- Screenshots at 1280x800 and 1600x1000 with `DEADTUNE_PREVIEW_IMAGES` set, read next to crops of the in-game screenshot, for every preview; and with `DEADTUNE_PREVIEW_SHAPES=1` for the fallback shapes.
- UI images page over the full export (release build, Mac): the first capture of the grid takes 1.1 to 1.5 s including start-up, the heroes folder 2.7 s; decoding is on the thumbnail workers, the frame stays interactive.
