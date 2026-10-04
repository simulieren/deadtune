# data/

`convar_catalog.csv` is generated from the downloaded presets. One row per convar that appears in the ConVars block of any preset.

- `apply_class` is derived from the flags in OptimizationLock's `cvars_we_can_modify.txt`:
  - `live`: settable from the in-game console at runtime
  - `live w/ sv_cheats`: needs cheats (hideout / sandbox / private lobby)
  - `restart (gameinfo only)`: `devonly` convars, only take effect via gameinfo.gi at launch
  - `unknown`: not in the dump (renamed, removed or newer than the dump)
- Preset columns: value set by that preset. A leading `//` means present but commented out. Empty means not mentioned.
- `preset_comment` is the first inline comment found next to the convar (mostly Sqooky's, often contains `[def: "x"]`).

Flags come from a community dump; treat them as a starting point and verify in game (Phase 0 of the plan).

Rows with no preset column set were added by hand for ConVars no preset uses but the app edits (the enemy UI colour ConVars, flags from `research/configs/OptimizationLock/convars.txt`).
