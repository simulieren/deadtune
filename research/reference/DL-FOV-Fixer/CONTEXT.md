# DL-FOV-Fixer

The words this project uses, so code, menus and documents say the same thing.

**FOV value**:
The `r_aspectratio` number the user chose, kept as the text they entered (`2` stays `2`, not `2.0`).
It is not an angle. Degrees are only ever an approximation shown beside it.
_Avoid_: FOV in degrees as a stored value, aspect ratio

**Apply**:
Write the FOV value, and the extra tweaks when they are enabled, into the game's files. Applying is
idempotent, and a file that already matches is not written.
_Avoid_: Patch, fix, install

**Drift**:
The file was correct and is not any more, which in practice means a game update overwrote it. Drift
is the normal case this app exists for, not a fault.
_Avoid_: Corruption, reset

**Extra tweaks**:
A whole config the user pasted once, stored as ordered key and value pairs and re-applied alongside
the FOV. Keys are routed by shape: `setting.*` to `cfg/video.txt`, PascalCase to `SceneSystem`,
anything else to `ConVars`.
_Avoid_: Profile, preset, settings

**Sub-block**:
A key inside `ConVars` that owns a block of its own, such as `rate` or `speaker_config`. A sub-block
is never rewritten, because a mangled one stops Deadlock from launching.
_Avoid_: Nested setting, group

**One-time backup**:
The first copy of a file the app is about to change, saved next to it as `<name>.dlfovfixer.bak` and
never overwritten afterwards, so the original is always the pre-app state.
_Avoid_: Snapshot, versioned backup

**Waiting**:
The game has the file open, so the write failed with a sharing violation. The app retries and keeps
its last known status instead of reporting an error.
_Avoid_: Locked error, permission denied

**Update channel**:
Where an installed copy looks for a newer version. Today the latest public GitHub Release of this
repository.
_Avoid_: Server, backend
