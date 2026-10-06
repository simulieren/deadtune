# Lower-left "Player stats" cluster (build 25712201)

Sources: decoded `text/panorama/{layout,styles}` (minified CSS, pretty-printed locally), decoded PNGs in `ui-images-25712201-2026-10-05/panorama/images`, reference shot `crates/dt-gui/assets/vanilla_hud.jpg` (1280x720; all pixel numbers below are x1.5 to 1920x1080).
Evidence tags: [V] read in the vanilla files, [M] measured on the screenshot, [I] inferred. No game scripts (JS) are in the dump, so classes toggled from native code/JS are inferred from the CSS that reacts to them.

## 0. Findings that change the plan

1. The three category numbers (79 / 161 / 4557) are NOT in `#hudPlayerStats`. They come from `CitadelHudActivePlayerStats#hudActivePlayerStats` (hud.xml line 237, a sibling of `#StatsAndModsContainer`, not inside `#LowerLeft`). `#hudPlayerStats` (`CitadelHudPlayerStats`, `#HudStatBlock` collapsed) is dead weight in vanilla. [V]
2. The green boxes with "+18", "+7", "+1.5" above the numbers are `.miniModifier.isPositive.shouldShow` popups of `#StatList` (same panel). The little beetle icon in each is `#heroIcon` in `#casterList`. [V CSS, M position]
3. The faint rounded square right of the item grid (x 397-460, y 990-1055 at 1080p) is `#HudMini` (quickbuy slot, `opacity: 0.2`) holding the favourite-star next item. `ModPurchasedPanelUniversalLocked` is empty in the shot. [M + V]
4. The big ruler gauge with "4557" at x 420-590, y 715-925 is not CSS-reachable from this cluster (world/other panel); ignore. [M, source unidentified]
5. The tiny black/white bar right of each number is `CitadelHudSingleItemBarGraph` inside `#BarGraphContainer`. `hud.css` has `#BarGraphContainer{visibility:collapse}` and `.gShopOpen #BarGraphContainer{visible}`, but `citadel_hud_active_player_stats.css` sets `#BarGraphContainer{...}` without visibility. The screenshot shows it, so the bar is visible out of shop (load order decides). [V + M]
6. `CitadelHudSoulAPContainer{visibility:collapse}`; `.ShowGoldAndAPOnHud` makes it visible (hud.css lines ~2491-2496). The game sets that class based on a setting; hiding via CSS needs only `visibility:collapse` on the id. [V]
7. Item tile colours are not CSS: tiles carry category colour in the item PNG (`images/items/{weapon,spirit,vitality,brawl,seasonal}/*_psd.png`, 200x200, semi-transparent colour baked in) [V file, I that it is the only source]. CSS only sets `.mod_icon_single_container{background-color:#00000030}`.

## 1. Panel tree

```
#StatsAndModsContainer.clamp_width            (hud.xml 294; GlobalClassListener: gEditingBuilds gShopOpen gScoreboardOpen)
 #LowerLeft
  CitadelHudPlayerStats#hudPlayerStats        (citadel_hud_player_stats.xml)
   #HudStatBlock                              (visibility:collapse in vanilla)
    #CoreStats.stat_block > #Weapon|#Spirit|#Health .core_stat > .core_bg, (.wpn_stat_area), Label#stat_label, (Label#delta_label)
    #SubStats.stat_block > .stat_column.weapon|spirit|armor > .sub_stat#BulletDamage|FireRate|Ammo|CooldownReduction|AbilityDuration|AbilityRange|Vitality|Regen|MoveSpeed|WeaponResist|SpiritResist
  CitadelStatusEffect#StatusEffects display_location=bottom_left   (citadel_status_effect.xml)
   #StatusEffectContainer > snippet .statusEffect > .immuneImage, Label#stacks, .statusEffectContainer > #StatusEffectsBorder, #StatusEffectCooldownOverlay, #StatusEffectInner, Image#StatusEffectImage, .statusEffectImage
  CitadelHudSoulAPContainer#gold_and_ap_container   (hud_gold_and_ap_container.xml; GCL: gShopOpen gStreetBrawl gPVE)
   #PlayerLevelContainer
    #ToNextPanel > Label.toNext
    CitadelPlayerLevel#PlayerLevel            (citadel_hud_player_level.xml; GCL: gScoreboardOpen gShopOpen)
     .PlayerLevelContainer > #SoulsFrame.SoulsLevelImages, #Souls > #SoulsFill.SoulsLevelImages, Label#PlayerLevelNumber
     #LevelAmount.ScalingStatImage > CitadelHeroImage#HeroImage
   #hudGoldContainer
    #CurrentGoldAmount > #hudCurGoldIcon, Image#hudGoldInfinite, Label#hudCurGoldLabel.CurrencyLabel, Label#hudGoldLabel.CurrencyLabel ("SOULS")
    .hudItemGoldContainer > #hudCurGoldIcon, Label#hudCurGoldLabel, Label#hudGoldLabel      (only with .infiniteMoney)
    .hudDeathGoldContainer > Image#hudDeathGoldIcon, Label#hudDeathGoldLabel.death_penalty_gold, Label#hudUnsecuredLabel.UnsecuredLabel
  #ModsContainer.ModsContainer
   .ModSection.FlexMods > CitadelModsPurchasedPanel#ModPurchasedPanelUniversal.mod_universal.modsButton (mod_category=Universal)
   .ModSection.FlexMods > CitadelModsPurchasedPanel#ModPurchasedPanelUniversalLocked.mod_universal.modsButton (mod_category="Locked Universal")
    each panel (citadel_mods_purchased_panel.xml): #ModIconsContainer > .ModIconColumn (snippet, added by code, flow down) > CitadelModIcon ...; #ScoreboardHoverTab > .modCategoryIcon
    CitadelModIcon (citadel_shop_mod_icon.xml)
     #modIconContainer.mod_icon_single_container
      #mod_icon_background.mod_icon_background_container > .mod_icon#mod_icon, Image#ModIconImage
      #TierContainer > .tier_bg, #mod_tier_label (.ModTierLevel1-5)
      #ItemHidden > Label "?"
      #ActiveTagContainer > Label#embedded_active_tag
      #Enhanced, #Corrupted, #CooldownMask
      #UpgradeLevelContainer > #UpgradeLevel > Label "+{upgradeLevel}"
  CitadelHudQuickbuy#CitadelHudQuickbuy         (layout hud_quickbuy.xml; class .HudQuickbuy; GCL: gStreetBrawl gQuickbuyShopShowQueue)
   .EmptyContents.HudQuickbuyElement > .PurchaseDisabledContainer, Label.QuickbuyLabel, .rightClick
   #HudMini.HudQuickbuyElement > #HudMiniContents > .PurchaseDisabledContainer(#DisabledCooldownMask), CitadelHudQuickbuyEntry#QuickbuyNext (> #ItemContentPanel > CitadelModIcon#ModIcon, .NamePanel), .ItemReadyIndicator, .QueueSize
   #KeyboardHints > Label.hudHint.itemName|combatText|buyKey|shopKey
   #QuickBuyQueueContainer > .QuickbuyQueueOuter > Label.BuildOrderLabel, Label.InstructionalText, #QuickbuySellQueue, #QuickbuyQueue, #QueueActions, Label.emptyQueueLabel
   .QuickbuyShopSummaryContainer > #QuickbuyShopSummary.QuickbuyShopSummary   (shop only)

#hudActivePlayerStats  (CitadelHudActivePlayerStats, class clamp_width; sibling of the container; GCL: gDetailView gScoreboardOpen gShopOpen gEditingBuilds gQuickbuyShopShowQueue gInCombat)
 #StatList > #WeaponColumn|#SpiritColumn|#VitalityColumn .column_stats > .miniModifier (#fireRateContainer, #clipSizeContainer, #bulletDamageContainer, ... .secondaryStat) > .miniModifierCore > .statIcon.PropertiesIcon.<X>, Label.statNumber, Label.statPostfix ; #casterList
 #HudStatBlock (visible here)
  #CoreStats.stat_block > #Weapon|#Spirit|#Vitality .core_stat
     > .core_bg, #BarGraphContainer > CitadelHudSingleItemBarGraph#WeaponBarGraph|SpiritBarGraph|VitalityBarGraph (slot_type Weapon|Tech|Armor)
     , Label#stat_label (collapsed, "000"), .miniModifier#weaponPowerContainer|#spiritContainer|#maxHealthContainer
        > .miniModifierCore > .statIcon, (.statWithPostfix > Label.statNumber + Label.statPostfix), Label.statNumberDelta ; Label.max_label ; #casterList
  #SubStats (visibility:collapse)
 #CoreStatFXLayer > ParticleScenePanel#{Weapon,Spirit,Vitality}{Spike,Max}StatFX.stat_fx
```
Bar graph panels (citadel_hud_single_item_bar_graph.xml): `#ProgressBarBorder`, `#ProgressBar.modsProgressBar`, `#Image.progressBarImage`; children `.ProgressBarLeft/.ProgressBarRight/.ProgressBarPreview` are created natively. Container variant `CitadelHudItemBarGraph > #BarsContainer > 3x CitadelHudSingleItemBarGraph` (`#WeaponBarContainer`, `#TechBarContainer`, `#ArmorBarContainer`) is used in shop/top bar, not in this cluster.
Duplicate ids in `hud_gold_and_ap_container.xml`: `#hudCurGoldIcon`, `#hudCurGoldLabel`, `#hudGoldLabel` each appear 2-3 times, so one rule hits the souls row, item-value row and unsecured row at once. Scope with `#CurrentGoldAmount #hudGoldLabel`.

## 2. Vanilla CSS per visible part

State classes (set by the game): on the root-level panels `gShopOpen`, `gScoreboardOpen`, `gDetailView`, `gEditingBuilds`, `gInCombat`, `gStreetBrawl`, `gPVE`, `gQuickbuyShopShowQueue`; on `.ModsContainer` descendants `.viewingWeaponStats/.viewingArmorStats/.viewingTechStats`; on `hud` root `.spec_mode`, `.player_selected`, `.replay_playback`, `.InHideout`, `.GameStatePreGame`, `.GameStatePostGame`, `.wants_scoreboard`, `.AspectRatio21x9`.

### 2.1 Container, #LowerLeft (hud.css)
```css
#StatsAndModsContainer,.gShopOpen.wants_scoreboard #StatsAndModsContainer{visibility:visible;width:100%;height:100%;ui-scale:100%;transition-property:ui-scale,transform,margin-bottom,margin-left;transition-duration:0.1s;transition-timing-function:ease-out;}
#StatsAndModsContainer.gShopOpen{margin-bottom:40px;}
#StatsAndModsContainer.gEditingBuilds{visibility:collapse;}
#LowerLeft{horizontal-align:left;vertical-align:bottom;padding-top:20px;height:100%;}
#LowerLeft .gShopOpen{margin-bottom:20px;}
.AspectRatio21x9 #StatsAndModsContainer{horizontal-align:center;}
.spec_mode #StatsAndModsContainer,.spec_mode #hudActivePlayerStats{visibility:collapse;}   /* .player_selected restores visible */
.GameStatePostGame ... #StatsAndModsContainer, #hudActivePlayerStats {opacity:0;}
.InHideout #StatsAndModsContainer {opacity:0;pre-transform-scale2d:.9;wash-color:dodgerblue;}  /* .gShopOpen restores */
#StatsAndModsContainer,#hudActivePlayerStats,... {transition-property:opacity,pre-transform-scale2d,wash-color;transition-duration:.21s;}
```
Community (qollock) proven: `.AspectRatio4x3 #StatsAndModsContainer.gShopOpen #LowerLeft{transform:translateX(10px) translateY(-10px);}`.

### 2.2 Category numbers: `#hudActivePlayerStats` (hud.css + citadel_hud_active_player_stats.css)
```css
#hudActivePlayerStats{width:100%;height:100%;visibility:visible;}   /* hud.css; .gEditingBuilds -> collapse */
CitadelHudActivePlayerStats{transition-property:position;width:100%;height:100%;}
#HudStatBlock{vertical-align:bottom;horizontal-align:left;flow-children:down;width:270px;margin-left:20px;margin-bottom:220px;}
.gShopOpen #HudStatBlock{margin-bottom:250px;}
#HudStatBlock #CoreStats{width:100%;visibility:visible;flow-children:right;}
#HudStatBlock #CoreStats .core_stat{width:80px;height:80px;margin:5px;transition-property:pre-transform-scale2d;transition-duration:0.1s;}
#HudStatBlock #CoreStats .core_stat:hover{pre-transform-scale2d:1.3;}
#HudStatBlock #CoreStats .core_bg{width:100%;height:100%;background-size:cover;ignore-parent-flow:true;opacity:0.5;}
#HudStatBlock #CoreStats #Weapon .core_bg{background-image:url("s2r://panorama/images/hud/core/core_weapon_icon_psd.vtex");wash-color:courageBrightColor;}
#HudStatBlock #CoreStats #Spirit .core_bg{background-image:url(".../core_spirit_icon_psd.vtex");wash-color:spiritBrightColor;}
#HudStatBlock #CoreStats #Vitality .core_bg{background-image:url(".../core_armor_icon_psd.vtex");wash-color:fortitudeBrightColor;}
#HudStatBlock #CoreStats #stat_label{visibility:collapse;}     /* the "000" label is hidden; the visible number is .statNumber */
#CoreStats .miniModifier{width:80px;height:80px;padding:0;margin:0;vertical-align:middle;}
#CoreStats .miniModifierCore{background-color:none;width:100%;height:40px;padding:0;vertical-align:bottom;flow-children:none;margin-bottom:4px;margin-right:10px;}
#CoreStats .has_delta .miniModifierCore{margin-bottom:15px;}
#HudStatBlock #CoreStats .statNumber{font-size:26px;font-family:numericOracle;font-weight:bold;text-shadow:0px 0px 0px 5.0 offBlack;vertical-align:middle;horizontal-align:middle;padding:2px;transform:rotateZ(-3deg);}   /* color:offWhite via #CoreStats .statNumber */
#HudStatBlock #CoreStats .statNumberDelta{font-size:14px;ignore-parent-flow:true;text-shadow:0px 0px 0px 3.0 offBlack;horizontal-align:right;visibility:collapse;margin-top:20px;}   /* .has_delta -> visible; .isPositive color:#8bf98b; .isNegative color:#fc8282 */
#CoreStats .statIcon{visibility:collapse;}            /* the small property icon is hidden in the core cells */
#HudStatBlock #CoreStats .max_label{font-size:12px;font-weight:bold;font-family:block;text-transform:uppercase;visibility:collapse;margin-top:20px;horizontal-align:right;}  /* .stat_state_max shows it; color per #Weapon/#Spirit/#Vitality = courageLight/spiritLight/fortitudeLight */
#BarGraphContainer{width:100%;height:100%;ignore-parent-flow:true;vertical-align:top;transform:translateY(-10px) translateX(4px);ui-scale:100%;}
#HudStatBlock CitadelHudItemBarGraph{height:54px;}
#HudStatBlock #SubStats{flow-children:right;visibility:collapse;}
#CoreStatFXLayer{width:300px;height:500px;ignore-parent-flow:true;vertical-align:bottom;margin-bottom:280px;z-index:-1;}
#CoreStatFXLayer .stat_fx{width:304px;height:550px;brightness:1;opacity:0;}   /* .weapon_stat_state_spike|max etc. -> opacity:0.7 */
#CoreStatFXLayer #WeaponSpikeStatFX,#WeaponMaxStatFX{x:-85px;y:270px;} /* Spirit x:5px, Vitality x:95px */
```
Popups `#StatList` (shown on stat change; this is the green "+18" box):
```css
#StatList{vertical-align:bottom;horizontal-align:left;flow-children:right;margin-bottom:280px;width:320px;height:600px;padding:30px;overflow:noclip;transition-property:opacity;transition-duration:0.1s;}
.gShopOpen #StatList{margin-bottom:320px;}   .gShopOpen.gQuickbuyShopShowQueue #StatList{opacity:0;}
#StatList .column_stats{flow-children:up;height:100%;width:33%;}
.miniModifier{margin-right:2px;margin-bottom:4px;height:32px;width:100%;opacity:0;padding-left:0;margin-left:0;}   /* .gDetailView/.gScoreboardOpen/.gShopOpen -> opacity:1; .alwaysShowStats/.shouldShow -> opacity:1;visibility:visible; .secondaryStat.isBaseValue|.isZero -> collapse */
.miniModifier.trigger_appear,.miniModifier.trigger_statchange{animation-name:buffApplied_new;animation-duration:0.6s;animation-iteration-count:1;opacity:1;}
.miniModifierCore{padding:0 4px;min-width:80px;border-radius:2px;flow-children:right;height:100%;}
#WeaponColumn .isPositive.shouldShow .miniModifierCore,(#Spirit..,#Vitality..){background-color:#84e184aa;wash-color:none;}
#WeaponColumn .isNegative .miniModifierCore,...{background-color:#ff876faa;wash-color:none;}
.statIcon{width:18px;height:18px;vertical-align:middle;margin-right:4px;background-size:contain;img-shadow:0px 0px 0px 4.0 offBlack&90;}
#WeaponColumn .statIcon{wash-color:courageColor&d9;} #SpiritColumn .statIcon{wash-color:spiritColor&d9;} #VitalityColumn .statIcon{wash-color:fortitudeLightColor&d9;}
.isPositive .statIcon,.isNegative .statIcon{wash-color:black;}
.statNumber{font-size:17px;font-family:sansMono;font-weight:bold;vertical-align:middle;}  .miniModifierCore Label.statNumber{font-family:numericOracle;}
.miniModifierCore Label{vertical-align:middle;color:#d3d3d3;font-family:oracle;font-weight:bold;text-shadow:0px 0px 0px 3.0 offBlack;padding:2px;}
.isPositive.shouldShow Label,...{wash-color:offBlack;font-size:16px;text-shadow:0px 0px 0px offBlack&00;}
.statPostfix{font-size:16px;opacity:0.5;vertical-align:bottom;padding:0;}
#casterList{flow-children:right;height:26px;z-index:10;ignore-parent-flow:true;min-width:30px;horizontal-align:right;vertical-align:bottom;margin-right:1px;margin-bottom:-6px;ui-scale:80%;}
#heroIcon{width:26px;height:26px;vertical-align:middle;margin-right:-3px;} #StatList #heroIcon{ui-scale:70%;}
```
Classes toggled by game on each `.miniModifier`: `isPositive`, `isNegative`/`IsNegative`, `isBaseValue`, `isZero`, `shouldShow`, `trigger_appear`, `trigger_statchange`, `has_delta` (on core cell), `stat_state_max`, `self`, `is_ability`; on `#CoreStatFXLayer`: `weapon_stat_state_spike|max`, `spirit_...`, `vitality_...`; on root: `alwaysShowStats`.

### 2.3 Level / AP badge (hud_gold_and_ap_container.css + citadel_hud_player_level.css)
```css
#gold_and_ap_container{horizontal-align:left;vertical-align:bottom;margin-left:20px;margin-bottom:140px;margin-top:30px;ui-scale:100%;width:400px;overflow:noclip;transition-property:ui-scale,transform,margin-bottom,margin-left,opacity;transition-duration:0.02s;transition-timing-function:ease-out;}
#gold_and_ap_container.gShopOpen{margin-left:20px;margin-bottom:150px;transform:translateX(0px) translateY(0px);visibility:visible;}
CitadelHudSoulAPContainer{visibility:collapse;}  CitadelHudSoulAPContainer.ShowGoldAndAPOnHud{visibility:visible;}   /* hud.css */
#PlayerLevelContainer{max-width:200px;horizontal-align:left;opacity:1;vertical-align:middle;flow-children:down;height:85px;}
CitadelPlayerLevel{height:60px;width:60px;horizontal-align:left;vertical-align:bottom;visibility:visible;opacity:1;z-index:2;transition-property:opacity;transition-duration:0.1s;}
.PlayerLevelContainer{opacity:1;height:120px;width:60px;vertical-align:bottom;}
.SoulsLevelImages{height:60px;width:60px;background-size:60px 60px;}
#SoulsFrame{background-image:url("s2r://panorama/images/hud/core/spirit_jar_frame_png.vtex");}
#Souls{height:60px;width:60px;vertical-align:bottom;horizontal-align:left;z-index:2;}
#SoulsFill{background-image:url(".../spirit_jar_fill_png.vtex");background-size:52px 51px;background-position:50% 100%;opacity:0.4;margin:4px 5px;height:51px;width:60px;background-repeat:no-repeat;vertical-align:bottom;z-index:2;}
#PlayerLevelNumber{text-align:center;horizontal-align:middle;vertical-align:middle;font-size:28px;z-index:4;margin-top:5px;margin-left:5px;text-shadow:0px 0px 10px #00000060;}   /* Label{font-family:sansMono;font-weight:bold} */
#LevelAmount{flow-children:right;horizontal-align:right;margin-top:38px;margin-right:35px;ignore-parent-flow:true;z-index:10;}
#ToNextPanel{margin-bottom:5px;opacity:0;} .gScoreboardOpen #ToNextPanel{opacity:1;} .gStreetBrawl #ToNextPanel{visibility:collapse;}
#ToNextPanel .toNext{font-size:14px;text-transform:uppercase;font-weight:bold;color:shardColor;opacity:0.6;vertical-align:bottom;}
```
The teal circle/bomb shape and its fill in the screenshot is the jar frame/fill PNG; the fill height changes with level progress (natively) [I].

### 2.4 Souls number and "SOULS" label (hud_gold_and_ap_container.css)
```css
#hudGoldContainer{flow-children:down;vertical-align:bottom;margin-bottom:8px;margin-left:68px;width:fit-children;}   /* shared rule with #HudCurrentAPContainer */
.gStreetBrawl #hudGoldContainer{visibility:collapse;}
#CurrentGoldAmount{flow-children:right;vertical-align:middle;}
#hudCurGoldIcon{horizontal-align:center;height:36px;width:20px;margin:3px;vertical-align:middle;background-image:url("s2r://panorama/images/hud/icons/icon_soul.vsvg");background-size:contain;background-repeat:no-repeat;wash-color:shardColor;img-shadow:0px 0px 0px 4.0 offBlack;}
.CurrencyLabel{font-size:32px;vertical-align:middle;color:blueAPTextColor;font-weight:bold;font-family:numericOracle;text-shadow:0px 0px 0px 4.0 offBlack;overflow:noclip;}
#hudGoldContainer .CurrencyLabel{color:shardColor;}
#hudGoldLabel{font-size:14px;text-transform:uppercase;color:offWhite;opacity:0.4;vertical-align:bottom;margin-bottom:5px;margin-left:5px;text-overflow:shrink;max-width:80px;max-height:26px;}
.hudDeathGoldContainer{visibility:collapse;flow-children:right;margin-top:0;margin-left:4px;}  /* shown by .death_penalty_gold_danger_level_1..4 on the container */
.hudDeathGoldContainer Label{font-size:14px;color:#ff9292;font-family:sansMono;text-transform:uppercase;...}  #hudUnsecuredLabel{color:colorEnemy;opacity:0.2;animation-name:unsecured_pulse;...}
.hudItemGoldContainer{visibility:collapse;...}  .infiniteMoney .hudItemGoldContainer{visible} .infiniteMoney #CurrentGoldAmount{collapse}
```
Animation classes on the container: `goldIncreaseEvent_Min|Med|Max`, `goldDecreaseEvent_Min|goldeDecreaseEvent_Med|goldDecreaseEvent_Max` (note vanilla typo `goldeDecreaseEvent_Med`) scale `.CurrencyLabel` via keyframes with `transform:scaleX()/scaleY()`, `color`, `text-shadow`. Setting `text-shadow` or `color` with plain rules is overridden only during the animation. [V]

### 2.5 Item grid (citadel_mods_purchased_panel.css + hud.css)
```css
.ModsContainer{horizontal-align:left;vertical-align:bottom;height:102px;width:fit-children;margin-bottom:16px;margin-left:22px;flow-children:right;visibility:visible;ui-scale:120%;}   /* hud.css */
.ModSection{flow-children:down;transition-property:background-color,border;transition-duration:0.2s;}   /* hud.css */
CitadelModsPurchasedPanel{margin:0 5px;flow-children:down;horizontal-align:middle;vertical-align:bottom;}
.ModsContainer CitadelModsPurchasedPanel{width:fit-children;height:102px;margin:0;}
#ModPurchasedPanelUniversalLocked{width:104px;}
#ModIconsContainer{vertical-align:middle;flow-children:right;horizontal-align:left;height:100%;}
#ModIconsContainer .ModIconColumn{flow-children:down;}
#StatsAndModsContainer .modCategoryName{visibility:collapse;}  #StatsAndModsContainer .modCategoryIcon{visibility:collapse;}
.modsBacker{... visibility:collapse;} .gShopOpen .modsBacker{visibility:visible;}   /* category coloured backer, shop only */
.gShopOpen .WeaponMods .modsBacker{background-color:courageDarkColor&20;border:2px solid courageBrightColor&30;} /* Armor=fortitude*, Tech=spirit* */
.ModTypeLabel{font-size:10px;text-transform:uppercase;font-weight:bold;horizontal-align:center;opacity:0.5;visibility:collapse;} .gShopOpen .ModTypeLabel{visible}
```
Grid shape: 2 rows (each `.ModIconColumn` flow-down holds 2 icons, created natively) x N columns (6 for the 12 universal slots in the shot); `ModPurchasedPanelUniversal` width is `fit-children`. [V + M]
Top-bar variant proves another flow: `CitadelHudTopBar #ModIconsContainer{horizontal-align:middle;width:70px;height:fit-children;flow-children:right-wrap;}` with `.ModIconColumn{flow-children:right;width:100%;}`.

### 2.6 Item tile (citadel_shop_mod_icon.css)
```css
CitadelModIcon{tooltip-body-position:50% 50%;overflow:noclip;padding:3px;}
.mod_icon_single_container{flow-children:none;width:45px;height:45px;background-color:#00000030;border-radius:3px;z-index:1;}
#StatsAndModsContainer .unowned.mod_icon_single_container{background-color:#ffffff05;opacity:0.8;}   /* empty / unbought slots */
.Locked .mod_icon_single_container{background-image:url("s2r://panorama/images/hud/shop_ability_upgrades_lock_icon_png.vtex");background-size:35% 45%;background-repeat:no-repeat;background-position:50% 50%;background-img-opacity:0.25;}
#StatsAndModsContainer .Locked .mod_icon_single_container{background-img-opacity:0.15;background-size:40% 50%;}
.mod_icon_background_container{width:100%;height:100%;vertical-align:center;horizontal-align:center;background-image:none;}
.mod_icon{visibility:collapse;height:100%;width:100%;margin:2px;background-size:100% 100%;}
#ModIconImage{height:100%;width:100%;visibility:collapse;} .hasAbility #ModIconImage{visible} .isActiveItem #ModIconImage{border-radius:50px;}
/* cooldown */
#CooldownMask{width:100%;height:100%;background-color:offBlack&ee;visibility:collapse;transform:scaleX(-1);}  .OnCooldown #CooldownMask{visible} .VerticalCooldown #CooldownMask{transform:scaleX(1);vertical-align:bottom;}
/* tier corner */
#TierContainer{width:28px;height:28px;horizontal-align:right;visibility:collapse;} .hasAbility #TierContainer{visible} .isTier5 #TierContainer{collapse}
.tier_bg{width:100%;height:100%;horizontal-align:right;background-image:url("s2r://panorama/images/shop/tier_corner_cap.vsvg");background-size:22px;background-repeat:no-repeat;background-position:right 0px;img-shadow:-4px 0px 10px #000000ee;}
.isWeapon .tier_bg{wash-color:courageBrightColor;} .isArmor .tier_bg{wash-color:fortitudeBrightColor;} .isTech .tier_bg{wash-color:spiritBrightColor;}
#mod_tier_label{horizontal-align:right;height:8px;background-size:100% 100%;margin-top:3px;z-index:2;wash-color:offBlack;opacity:1;}  .HideModTierLabel #mod_tier_label{collapse}
#mod_tier_label.ModTierLevel1..5 {background-image:url(".../upgrades/tier_numbers_small_0N.vsvg");width:5/7/9/11/12px;margin-right:4/3/2/1/1px;}
/* charges / upgrade pip */
#UpgradeLevelContainer{width:28px;height:28px;horizontal-align:left;vertical-align:bottom;visibility:collapse;margin-left:-5px;margin-bottom:-3px;} .hasUpgradeLevel #UpgradeLevelContainer{visible}
#UpgradeLevel{width:100%;height:100%;background-image:url("s2r://panorama/images/hud/hero_ability_pip_png.vtex");background-repeat:no-repeat;background-size:100%;}
#UpgradeLevel Label{font-size:17px;font-family:sansMono;font-weight:bold;text-shadow:0px 0px 0px 4.0 offBlack;color:greenyellow;horizontal-align:center;vertical-align:center;}
/* active tag, hidden item, enhanced, corrupted */
#embedded_active_tag{... visibility:collapse;} .isActiveItem #embedded_active_tag{visible}  #LowerLeft .isActiveItem #embedded_active_tag{visibility:collapse;}   /* already hidden in this cluster */
#ItemHidden{width:100%;height:100%;background-color:offBlack;visibility:collapse;} .isHidden #ItemHidden{visible}
#Enhanced{...visibility:collapse;background-color:gradient(linear,0% 60%,60% 0%,from(enhancedColorDark&90),to(enhancedColorDark&00));background-image:url(".../shop/enhanced_arrow_png.vtex");background-size:16px 20px;background-position:right bottom;} .isEnhanced #Enhanced{visible}
#Corrupted{...visibility:collapse;background-image:url(".../shop/corrupted_items/item_frame_corrupted_psd.vtex");background-size:100% 100%;} .isActiveItem #Corrupted{ ..._active_psd.vtex} .isCorrupted #Corrupted{visible}
CitadelModIcon.IsNewItem{animation-name:isNewItem;animation-duration:1s;animation-iteration-count:3;}   /* brightness flash */
CitadelModIcon.newSlotUnlocked{animation-name:FlexSlotUnlocked;animation-duration:1s;...}   /* pre-transform-scale2d to 2 */
```
Tile state classes (set natively): `isWeapon/isArmor/isTech`, `isTier0-5`, `hasAbility`, `isActiveItem`, `OnCooldown`, `VerticalCooldown`, `unowned`, `Locked`, `isHidden`, `isEnhanced`, `isCorrupted`, `hasUpgradeLevel`, `IsNewItem`, `newSlotUnlocked`, `HideModTierLabel`, `ModTierLevel1-5`.
"Charges": no dedicated charge label in the tile; the only count is `#UpgradeLevel` ("+N" on a green pip) [V]. Cooldown is a dark mask, no number label in this layout [V].

### 2.7 Quickbuy mini slot (hud_quickbuy.css, hud_quickbuy_entry.css)
```css
.HudQuickbuy{height:100%;}   .gShopOpen .HudQuickbuy{width:800px;}   .gEditingBuilds .HudQuickbuy{visibility:collapse;}
.gShopOpen .QuickbuyQueueOuter,.HudQuickbuyElement{vertical-align:bottom;horizontal-align:left;margin-left:390px;margin-bottom:0;border-radius:4px;}
.gShopOpen .HudQuickbuyElement{margin-left:540px;margin-bottom:22px;opacity:1;}
#HudMini{opacity:0.2;width:74px;height:94px;vertical-align:bottom;}   .ItemsReady #HudMini{opacity:1}  .NextIsSell #HudMini{opacity:0.5}  .gShopOpen #HudMini{visibility:collapse}  .Empty #HudMini{visibility:collapse}  .viewing_as_player.dead #HudMini{collapse}
#HudMini #HudMiniContents{width:70px;height:70px;border:3px solid offWhite&30;border-radius:5px;margin:4px;}
.ItemsReady #HudMini #HudMiniContents{opacity:1;border:3px solid shardColor;background-color:shardColor&60;animation-name:quickbuy_pulse;animation-duration:1.3s;animation-iteration-count:infinite;}
.purchasingDisabled #HudMini{animation-name:none;border:3px solid offWhite&30;background-color:#34343466;}
#HudMini .CostPanel{visibility:collapse;}
.EmptyContents{width:80px;height:80px;opacity:0.5;visibility:collapse;border:2px solid offWhite&10;...}   /* Empty state, hidden in vanilla */
#QuickBuyQueueContainer{padding-bottom:110px;vertical-align:bottom;}  .Empty #QuickBuyQueueContainer{collapse}
.QuickbuyQueueOuter{visibility:collapse;...}  .ShowQueue .QuickbuyQueueOuter{visible;max-height:500px}
.QuickbuyShopSummary{... visibility:collapse;}  .gShopOpen .QuickbuyShopSummary{visible}
```
States: `ItemsReady`, `NextIsSell`, `purchasingDisabled`, `Empty`, `ShowQueue`, `CanAffordCumulative`, `gShopOpen`, `gQuickbuyShopShowQueue`, `gEditingBuilds`, `viewing_as_player`, `dead`.

### 2.8 Status effects (citadel_status_effect.css, unit_status_icons.css)
```css
CitadelStatusEffect{height:32px;width:fit-children;vertical-align:top;horizontal-align:center;}
#LowerLeft CitadelStatusEffect{margin-bottom:200px;horizontal-align:left;margin-left:20px;vertical-align:bottom;}
.gShopOpen CitadelStatusEffect{visibility:collapse;}
#StatsAndModsContainer .immuneImage,#StatsAndModsContainer .modifierImmunity .immuneImage{visibility:collapse;}   /* unit_status_icons.css */
```
Entries use the `StatusEffect` snippet; icons/borders/stacks are in `unit_status_icons.css` (not dumped in detail here; none visible in the shot). [V layout, not styled further]

### 2.9 Bar graph (citadel_hud_item_bar_graph.css, hud.css)
```css
#BarGraphContainer{height:100%;width:80px;margin:2px;visibility:collapse;}  .gShopOpen #BarGraphContainer{margin-right:5px;width:68px;visibility:visible;}   /* hud.css */
#BarGraphContainer{width:100%;height:100%;ignore-parent-flow:true;vertical-align:top;transform:translateY(-10px) translateX(4px);}   /* active_player_stats.css */
.progressBarContainer{width:22px;height:100%;flow-children:down;padding:5px 3px 0 3px;tooltip-position:bottom;}
.modsProgressBar{width:10px;height:70%;horizontal-align:center;vertical-align:middle;margin-top:3px;box-shadow:0px 0px 0px 6.0 black;border-radius:1px;}  .gShopOpen .modsProgressBar{width:10px;height:74%;vertical-align:top;margin-top:0;}
.progressBarImage{height:12px;width:12px;opacity:0.4;visibility:collapse;margin-top:5px;} .gShopOpen .progressBarImage{visible}
.WeaponBar .ProgressBarLeft,.WeaponBar .ProgressBarPreview{background-color:gradient(linear,0% 0%,100% 0%,from(courageBrightColor),to(courageLightColor));}  /* ArmorBar = fortitude*, TechBar = spirit* */
.WeaponBar .ProgressBarRight{background-color:#221c08;} .ArmorBar .ProgressBarRight{#132b09} .TechBar .ProgressBarRight{#1a0a27}
.Horizontal .percentBar{width:0.5px;height:100%;}  .percentBar:nth-child(5){background-color:#dcd7d7;width:1px;opacity:1;}
#ProgressBarBorder{background-image:url(".../hud/stats_core_bar_border.vsvg");visibility:collapse;...}
#StatsAndModsContainer CitadelHudItemBarGraph{height:100%;horizontal-align:middle;}
```
`.ProgressBarPreview` pulses (`previewBarAnim`) while hovering a shop item; `.previewSell .ProgressBarPreview{background-color:redFlat}`.

## 3. Pixel geometry at 1920x1080 (origin bottom-left; x right, y up from the screen bottom)

Computed from CSS and checked against the screenshot (agreement within ~3 px). Tile maths: `CitadelModIcon` 45 + 2x3 padding = 51, x `ui-scale 1.2` = 61.2 pitch; drawn tile 45x1.2 = 54.

| Part | x (left..right) | y from bottom (bottom..top) | Size | Source |
|---|---|---|---|---|
| Item tile row 1 (top) | col i: 25.6+61.2*i, width 54-55 (6 cols: 25..388) | 80.5..134.5 up (top edge at 946 px from the top) | 55x55 | [M] x 25..80 first tile, 6th ends 387.5; row tops at 946 / 1007.5 from the top |
| Item tile row 2 | same x | 19.5..74 up | 55x55 | [M] 1060.5 from top = 19.5 up |
| Item grid box (both rows) | 25..388 | 19.5..134 | 363x114 | [M] |
| `.ModsContainer` (CSS) | x 22, width fit (367 + locked 125) | bottom 16, height 102x1.2 = 122 | | [V] |
| Locked panel | begins right of Universal (~x 389), 125 wide | same rows | empty in shot | [V] |
| Quickbuy `#HudMini` (faint) | 394..464 (box measured 397..460) | measured 25..90 up (1055..990 from top) | 74x94 element, 70x70 content | [M + V] |
| Level badge circle | 26..80 (jar 60x60 at x 20+) | 144..188 (measured 892..936 from top) | 54-60 square | [M] |
| Souls icon "$" | 91..106 | centred at ~y 168 | 20x36 | [M + V] |
| Souls number "4,748" | 91..191 | 153..185 from bottom (895..927 from top) | font 32 px, cap ~32 | [M] |
| "SOULS" label | 196..235 | ~160..169 | font 14 px | [M] |
| Core cell Weapon (79) | cell 25..105; number centre x~52-65, y~260 up | blob 248..303 up (777..832 from top) | cell 80x80, pitch 90 | [M + V] |
| Core cell Spirit (161) | cell 115..195; number centre x~150, y~249 up | blob 250..315 up | 80x80 | [M] |
| Core cell Vitality (4557) | cell 205..285; number x 209..266, y 240..258 up | blob 250..295 up | 80x80 | [M] |
| Delta "18" | x~85, y~254 up | | font 14 px | [M] |
| Mini bar graph | x 80..97 (weapon) | y 255..295 up | ~10 px wide bar | [M] |
| Green popup "+18" | x 30..107 | 385..418 up (662..695 from top) | ~77x33 | [M] |
| Green popup "+7" | x 30..107 | 312..347 up | ~77x35 | [M] |
| Green popup "+1.5" (boot) | x 202..283 | 420..450 up (630..660 from top) | ~81x30 | [M] |
| Status effects row (none in shot) | x 20.. | bottom margin 200, height 32 | | [V] |
| `#StatList` (popups area) | x 0..320 (+30 padding) | margin-bottom 280, height 600 | 320x600 | [V] |
| `#CoreStatFXLayer` | x -85/5/95 + 304 | margin-bottom 280 | 300x500 | [V] |

(Positions "from top" in the screenshot are y_top; y from bottom = 1080 - y_top.) Layout mock formulas that follow from the CSS:
- Core cells: left 20 + 5 margin, pitch 90, bottom of block at 220 up (250 with shop), cells 80 tall.
- Souls row: container bottom at 140 up (150 in shop), `#hudGoldContainer` left 20+68, bottom margin 8, badge 60x60 left 20.
- Item grid: left 22, bottom 16, scale 1.2.
- Quickbuy mini: left 390, bottom 0 (22 in shop; hidden in shop).

## 4. Images (game path -> decoded file under `.../panorama/images/`)

| Use | Game path (compiled `.vtex_c` / `.vsvg_c`) | Decoded | Size |
|---|---|---|---|
| Weapon core icon | `panorama/images/hud/core/core_weapon_icon_psd.vtex_c` | `hud/core/core_weapon_icon_psd.png` | 71x72 |
| Spirit core icon | `.../hud/core/core_spirit_icon_psd.vtex_c` | `hud/core/core_spirit_icon_psd.png` | |
| Vitality core icon | `.../hud/core/core_armor_icon_psd.vtex_c` | `hud/core/core_armor_icon_psd.png` | |
| Category glyphs (bar graph, mods) | `hud/core/icon_courage.vsvg_c`, `icon_fortitude`, `icon_magic`, `icon_spirit` | `hud/core/icon_courage.png/.svg`, `icon_fortitude`, `icon_magic`, `icon_spirit` | 256 |
| Level jar frame / fill | `hud/core/spirit_jar_frame_png.vtex_c`, `spirit_jar_fill_png.vtex_c` | `hud/core/spirit_jar_frame_png.png` (240x244), `spirit_jar_fill_png.png` (208x212) | |
| Souls icon | `hud/icons/icon_soul.vsvg_c` | `hud/icons/icon_soul.png/.svg` | 134x256 |
| Unsecured icon | `hud/icons/icon_unsecured_png.vtex_c` | `hud/icons/icon_unsecured_png.png` | |
| Infinity (cheats) | `upgrades/infinity_psd.vtex_c` | `upgrades/infinity_psd.png` | |
| Item art (passive/active) | `images/items/{weapon,spirit,vitality,brawl,seasonal}/<name>_psd.vtex_c` | same path `.png` (200x200, colour baked in alpha) | |
| Tier corner | `shop/tier_corner_cap.vsvg_c` | `shop/tier_corner_cap.png/.svg` | 256 |
| Tier numerals | `upgrades/tier_numbers_small_01..05.vsvg_c` | `upgrades/tier_numbers_small_0N.png` | 112x256 |
| Upgrade pip | `hud/hero_ability_pip_png.vtex_c` | `hud/hero_ability_pip_png.png` | 23x23 |
| Lock icon (locked slot) | `hud/shop_ability_upgrades_lock_icon_png.vtex_c` | `hud/shop_ability_upgrades_lock_icon_png.png` | 24x32 |
| Enhanced arrow | `shop/enhanced_arrow_png.vtex_c` | `shop/enhanced_arrow_png.png` | 55x72 |
| Corrupted frames | `shop/corrupted_items/item_frame_corrupted_psd.vtex_c`, `..._active_psd.vtex_c` | same `.png` | 300x300 |
| Bar graph border | `hud/stats_core_bar_border.vsvg_c` | `hud/stats_core_bar_border.png` | 256x81 |
| Quickbuy mouse glyph | `glyphs/mouse2.vsvg_c` | `glyphs/mouse2.png` | |
| Shard icon (shop quickbuy) | `hud/core/icon_shards.vsvg_c` | `hud/core/icon_shards.png` | |
| Property icons (SubStats, collapsed) | `AbilityPropertyIcon PropertiesIcon <Gun|FireRate|...>` from `ability_property_icons.css` | not traced | |

Tile "frame" is the item PNG itself (no separate frame image); a preview should draw the item PNG at 54x54 with `#00000030` backing, then the tier corner (22 px, wash-colour per category) top-right and the roman numeral (`tier_numbers_small_0N`, 8 px high, wash `offBlack`) on it.

## 5. @define values (citadel_base_styles.css)

| Name | Value | Used for |
|---|---|---|
| courageBrightColor | #EC9719 | weapon core icon wash, tier corner, bar, tile backer border |
| courageLightColor | #FFD18D | weapon max label, bar gradient end |
| courageColor | #9D620B | weapon statIcon wash (`&d9`) |
| courageDarkColor / courageDarkerColor | #7F540E / #302306 | shop backer fills |
| spiritBrightColor | #CE90FF | spirit core icon, tier corner, bar |
| spiritLightColor | #E7C8FF | spirit max label, AP label |
| spiritColor / spiritDarkColor / spiritDarkerColor | #8A55B3 / #613484 / #362147 | |
| fortitudeBrightColor | #7BBA1D | vitality core icon, tier corner, bar |
| fortitudeLightColor | #D3FC96 | vitality statIcon wash, max label |
| fortitudeColor / fortitudeDarkColor / fortitudeDarkerColor | #649717 / #4C7113 / #1F3400 | |
| shardColor | #99FFD6 | souls number, jar label, quickbuy ready border |
| blueAPTextColor | #80EEFF | `.CurrencyLabel` default (overridden by shardColor in `#hudGoldContainer`) |
| spiritLightColor on AP | `#HudCurrentAPContainer .CurrencyLabel` | |
| offWhite / offBlack | #FFEFD7 / #10130D | numbers / shadows |
| colorEnemy | #FF410D | decrease flash, unsecured label |
| colorGold | #FFED79 | max gold increase flash |
| enhancedColorDark / Bright / Light | #0E8FAC / #3FDCFF / #94F4FF | enhanced item arrow |
| ingameHudBlur | gaussian(2,2,2) | quickbuy panels (`world-blur`) |
| Fonts | `numericOracle` = Gulim, VALVEOracle, Reaver, Noto Sans, sans-serif; `sansMono` = Retail Text Demo, Noto Sans; `oracle` = VALVEOracle, Reaver, sans-serif; `block` = VALVEPulp, Noto Sans | |
| Local defines (mods panel) | weaponColor #FFA826 (also #E58A00 in base), armorColor #00FF99, techColor #00DDFF, *BGColor1..4 | shop category tabs, unused in this cluster |

Hex with alpha suffix `&XX` after a define (`courageBrightColor&30`) is vanilla syntax. Note `fortitudeLightColor` is the real name for vitality, but the 4557 green blob uses fortitudeBrightColor.

## 6. Customisation knobs (CSS appended to the game stylesheet)

Rules go in the file where the vanilla rule lives; same selector plus higher specificity or later position wins. "Proven" = a property the vanilla CSS already uses on that selector.

| Sub-part | Selector (file) | Knob | CSS | Risk / proven |
|---|---|---|---|---|
| Whole cluster | `#StatsAndModsContainer` (hud.css) | hide | `visibility:collapse;` | safe; proven (gEditingBuilds rule) |
| | | opacity | `opacity:0.6;` | safe; proven (`opacity:0` in GameStatePostGame) |
| | | scale | `pre-transform-scale2d:0.9;` or `ui-scale:90%;` | safe; both proven on this id |
| | | move | `transform:translateX(Npx) translateY(Npx);` or `margin-left/bottom` | safe; proven (qollock) |
| Numbers block | `#hudActivePlayerStats` (hud.css) | hide | `visibility:collapse;` | proven; also hides popups and bar |
| | `#HudStatBlock` (active_player_stats.css) | move | `margin-left:Npx; margin-bottom:Npx;` | proven (`margin-bottom:220px`, shop 250px; `.gShopOpen` rule must be overridden too) |
| | | width | `width:270px` | low |
| | `#HudStatBlock #CoreStats .core_stat` | cell size / spacing | `width/height:Npx; margin:Npx;` | proven; the icon, number and bar all scale with the cell except the number font |
| | | scale | `pre-transform-scale2d:1.2;` | proven (hover uses it) |
| | `#HudStatBlock #CoreStats .core_bg` | icon opacity / colour | `opacity:0.5; wash-color:...;` | proven; per id: `#Weapon .core_bg` etc. |
| | | hide icon | `visibility:collapse;` | safe |
| | `#HudStatBlock #CoreStats .statNumber` | font size / colour / shadow | `font-size:26px; color:#fff; text-shadow:0px 0px 0px 5.0 offBlack;` | proven; colour only proven via `#CoreStats .statNumber{color:offWhite}`; `.isPositive/.isNegative .statNumber` rules set colour too |
| | | tilt | `transform:rotateZ(0deg);` | proven (vanilla -3deg) |
| | `#CoreStats .statNumberDelta` | hide / size | `visibility:collapse;` `font-size:14px;` | proven; `.has_delta` rule re-shows it, so use `#HudStatBlock #CoreStats .has_delta .statNumberDelta{visibility:collapse}` |
| | `#BarGraphContainer` | hide / move | `visibility:collapse; transform:translateY(-10px) translateX(4px);` | proven (hud.css uses visibility, shop re-shows) |
| | `.max_label` | hide | `visibility:collapse` (also remove animation) | low |
| | `#CoreStatFXLayer` | hide particles | `visibility:collapse;` or `opacity:0` | low; proven opacity |
| Popups | `#StatList` | hide all | `opacity:0;` (proven, used in shop) or `visibility:collapse;` | safe; hides gain popups |
| | `#StatList .miniModifierCore` | colour | `background-color:#84e184aa;` per column/state | proven |
| | `.miniModifierCore Label` | font | `font-size`, `color`, `text-shadow` | proven |
| | `#StatList` | position | `margin-bottom:280px;` | proven |
| Level badge | `CitadelPlayerLevel` (player_level.css) | hide | `visibility:collapse;` | proven (`visibility:visible`) |
| | | scale | `pre-transform-scale2d:1.2;` or `ui-scale` | proven (ui-scale in container) |
| | `#PlayerLevelNumber` | font / colour | `font-size:28px; color:...;` | proven; colour not set in vanilla (inherits) |
| | `#SoulsFrame`, `#SoulsFill` | wash / opacity | `wash-color:#...; opacity:0.4;` | `opacity` proven on fill; `wash-color` valid game-wide |
| | `#PlayerLevelContainer` | whole badge area | `visibility:collapse;` | collapses `#ToNextPanel` too |
| Souls | `#gold_and_ap_container` (gold_and_ap.css) | move | `margin-left/margin-bottom` | proven, includes `.gShopOpen` override (150px) |
| | | scale | `ui-scale:100%;` | proven |
| | | hide | `visibility:collapse;` (already toggled via `CitadelHudSoulAPContainer`) | safe |
| | `#CurrentGoldAmount #hudCurGoldLabel` | number size/colour/shadow | `font-size:32px; color:#99FFD6; text-shadow:0px 0px 0px 4.0 offBlack;` | proven; animations override while gold changes |
| | `#CurrentGoldAmount #hudGoldLabel` ("SOULS") | hide / opacity / size | `visibility:collapse; opacity:0.4; font-size:14px;` | proven (opacity, font-size); `max-width:80px` clips big text |
| | `#CurrentGoldAmount #hudCurGoldIcon` | hide / wash | `visibility:collapse; wash-color:#99FFD6;` | proven (wash-color, size) |
| | `#hudGoldContainer` | offset | `margin-left:68px; margin-bottom:8px;` | proven |
| | `.hudDeathGoldContainer` | hide unsecured row | `.death_penalty_gold_danger_level_N .hudDeathGoldContainer{visibility:collapse;}` for N 1-4 (vanilla sets visible there) | specificity-sensitive |
| Item grid | `.ModsContainer` (hud.css) | scale | `ui-scale:120%;` | proven, qollock uses 90% |
| | | move | `margin-left:22px; margin-bottom:16px;` | proven |
| | | hide | `visibility:collapse;` | proven |
| | `.ModsContainer` | tile spacing | no direct gap; spacing = `CitadelModIcon{padding:3px}` (change to `padding:Npx`) | proven on `CitadelModIcon`; container `height:102px` must follow (2 x (45+2N)) or rows clip |
| | `.mod_icon_single_container` | tile size | `width:45px; height:45px;` | proven; combine with the padding/height note above; `ModPurchasedPanelUniversalLocked{width:104px}` should follow |
| | `.mod_icon_single_container` | tile backing | `background-color:#00000030; border-radius:3px;` | proven |
| | `#StatsAndModsContainer .unowned.mod_icon_single_container` | empty slots | `opacity:0.8; background-color:#ffffff05;` or `visibility:collapse` | proven; collapse changes flow |
| | `#ModPurchasedPanelUniversalLocked` | hide locked list | `visibility:collapse;` | safe; proven id |
| | `#ModIconsContainer` | wrap/column count | `flow-children:right-wrap; width:Npx;` plus `.ModIconColumn{flow-children:right}` | risky; proven only in top-bar variant, height then needs `fit-children` |
| Tile parts | `#TierContainer`, `.tier_bg` | hide tier mark | `visibility:collapse;` | proven |
| | `.isWeapon .tier_bg` etc. | corner colour | `wash-color:...;` | proven |
| | `#mod_tier_label` | numeral | `visibility:collapse;` or `wash-color` | proven |
| | `#UpgradeLevelContainer`, `#UpgradeLevel Label` | hide / size / colour | `visibility:collapse; font-size:17px; color:greenyellow;` | proven |
| | `#CooldownMask` | cooldown tint | `background-color:offBlack&ee;` | proven (quickbuy variants use `courageBrightColor&55`) |
| | `.OnCooldown #CooldownMask` | hide the dark mask | `visibility:collapse` | functional loss |
| | `.mod_icon_single_container` | outline | `border:2px solid #fff;` | `border` proven in other panels |
| | `CitadelModIcon:hover` | hover scale | `pre-transform-scale2d:1.2;` | proven (top bar) |
| | `CitadelModIcon` | per-tile brightness/saturation | `brightness:0.8; saturation:0.5;` | proven (`brightness`, `saturation` used on mod icons) |
| Quickbuy mini | `#HudMini` | hide | `visibility:collapse;` | proven |
| | | opacity | `opacity:0.2;` | proven; `.ItemsReady #HudMini` overrides to 1 |
| | `.HudQuickbuyElement` | move | `margin-left:390px; margin-bottom:0;` | proven; shop variant is separate (540px / 22px) |
| | `#HudMini #HudMiniContents` | size / border | `width:70px;height:70px;border:3px solid offWhite&30;` | proven |
| Status effects | `#LowerLeft CitadelStatusEffect` | move | `margin-bottom:200px; margin-left:20px;` | proven |
| | | hide | `visibility:collapse;` | proven (`.gShopOpen` rule) |
| Bar graph | `.modsProgressBar` | width / height / colour | `width:10px; height:70%;` | proven |
| | `.WeaponBar .ProgressBarLeft` | colour | `background-color:...;` | proven |

Risk notes:
- `flow-children`, `ignore-parent-flow`, `width:fit-children` on `#ModIconsContainer`, `.ModsContainer`, `#HudStatBlock #CoreStats`: layout depends on them; do not change.
- Tile size: the 2-row panel height is fixed at 102px (`.ModsContainer`, `.ModsContainer CitadelModsPurchasedPanel`); with other tile sizes both must be set to `2*(tile+2*padding)`.
- Rules that exist in two places (base rule and `.gShopOpen` rule, `#HudStatBlock` margin-bottom 220/250, `#gold_and_ap_container` 140/150, `#StatList` 280/320, quickbuy 390/540) need both overridden or the cluster jumps when the shop opens.
- Duplicate ids in the gold container: scope selectors.
- Animated properties (gold labels `transform`, `.miniModifier` `buffApplied_new` using `pre-transform-scale2d`/`transform`): a static rule on the same property is overridden for the animation duration.
- `#hudActivePlayerStats` is also the parent of popups; collapsing it kills both numbers and gain popups. Collapse `.core_stat` children and `#StatList` separately if the user wants only one.
- `#HudStatBlock` is two different panels: in `#hudPlayerStats` (collapsed, 300px wide) and in `#hudActivePlayerStats` (270px). Scope with `#hudActivePlayerStats #HudStatBlock`.
- Not verified in game: nothing in this file has been tested in the client; "proven" means vanilla CSS uses the property on that selector.

## 7. Community mod references (research/hud)

- `research/hud/web.md` L202-205, L213: panel/file map; mntbliss QoL HUD ships `hud_clear_inventory.css` (name suggests clearing the item grid chrome; contents not decoded) and edits `hud_gold_and_ap_container.xml` and `hud.xml`. [I]
- `research/hud/vpk-and-compiled-resources.md` L148 (cluster table), L171 (qollite: `.AspectRatio4x3 #StatsAndModsContainer.gShopOpen #LowerLeft{transform:translateX(10px) translateY(-10px);}`), L197 (qollock `ql_feat_aspect_ratio.css`):
  - `.support_16_10_active #hudPlayerStats{vertical-align:bottom;horizontal-align:left;flow-children:down;margin-bottom:270px;width:280px;transform:translateY(7%) translateX(-1.1%);}`
  - `.support_16_10_active .ModsContainer{horizontal-align:left;vertical-align:bottom;height:102px;width:380px;margin-bottom:16px;margin-left:16px;flow-children:right;ui-scale:90%;}`
  - `ql_feat_stats_position.css`: `#hudPlayerStats.QolStatsRight{horizontal-align:right;width:fit-children;}` (stats mirrored to the right via a class set by JS).
- L205: qollock JS positions the cluster with `FindChildTraverse("StatsAndModsContainer").style.x/y` (equivalent to `transform:translate`), L209: property inventory (`x`, `y`, `ui-scale`, `opacity`, `wash-color`, `margin-*`, `visibility`, `flow-children`, `transform`, `pre-transform-scale2d`, `overflow:noclip`, `z-index`).
- Blur Disabler proves `@define` overrides after `@import` work (L211) — relevant for `shardColor`, `courageBrightColor`, etc.: one `@define` change recolours every use of that colour in the stylesheet it is placed in (core icons, tier corners, bars, souls number), but only for that stylesheet's own rules. [I for this cluster]
- grep of `research/` for `StatsAndMods`, `gold_and_ap`, `ModsPurchased` returns only `web.md` and `vpk-and-compiled-resources.md`.

## 8. Suggested GUI page structure (not implemented)

Groups: Whole cluster (visibility, opacity, scale, offset) / Category numbers (visibility, cell size, number size and colour, icon opacity and tint, hide delta, hide mini bar, hide gain popups) / Level badge (visibility, scale, number size) / Souls (visibility, number size and colour, hide SOULS label, hide icon, offset) / Items (scale, tile padding, tile backing opacity, hide tier corner, hide numerals, hide locked panel, hide empty slots) / Quickbuy mini (visibility, opacity) / Status effects (offset, hide).
Preview needs: screenshot crop (x 0..480, y 0..460 up) as backdrop, or mock pieces from section 3 and PNGs from section 4.
