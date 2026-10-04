# Changelog

Every release, newest first. The version lives in `version.properties`, and a release is a Git tag
`vX.Y.Z` that matches it.

## Unreleased

- The tray shell now uses the shared `DotNetLib.Tray` package for its icon, menu, theming, dialogs
  and single-instance lock. The menu, the icon and the warm colors look the same. The paste dialog's
  button now says "OK" instead of "Import".

- The Python 1.x source, its tests and its release workflow are removed. 1.0.2 stays downloadable
  from its release, and the source is in the Git history at tag `v1.0.2`.

## 2.0.0 (2026-09-30)

The C# rewrite. It replaces the PyInstaller exe that Microsoft Defender reported as a trojan.

- A self-contained .NET 10 WPF tray app, delivered as a per-user Inno Setup installer in
  `%LOCALAPPDATA%\Programs\DL-FOV-Fixer`. It replaces a 1.x copy in that folder and keeps
  `%APPDATA%\DL-FOV-Fixer\config.json`, the Run value and the one-time backups.
- Re-applies the fix within seconds of a game update, through a file watcher, with the periodic
  check as the fallback.
- Six states behind the three icon colors. A running game that holds the file open is amber
  (waiting), not red.
- Light and dark themes that follow Windows, or a theme picked in the tray menu.
- Updates itself. Each release's `manifest.json` is signed with the project's key, and the app
  installs only an installer whose size and SHA-256 match the signed manifest. No code-signing
  certificate, so SmartScreen asks once on a browser download.
- Uninstall stops a running copy first, and leaves the settings in place.

## 1.0.2 (2026-09-30)

- The updater installs only an asset named exactly `DL-FOV-Fixer.exe`. A newer release without one,
  like the 2.x installer, is announced with a link to its release page, so a 1.x copy never copies
  an installer over itself.
- A pasted tweak whose key also exists inside a nested block, like `default` under `rate`, is added
  at the top of the block instead of overwriting the nested value. 2.0.0 does the same.

## 1.0.0 (2026-09-01)

- The first public release: a Python tray app that finds Deadlock's `gameinfo.gi` through Steam,
  keeps `r_aspectratio` in its `ConVars` block, re-applies it after game updates, and makes a
  one-time backup.
- A status-colored tray icon, pasted extra tweaks kept across updates, start with Windows, and a
  GitHub release updater.
