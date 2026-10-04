# DL-FOV-Fixer architecture

This describes the app as it ships today: a C# tray app on .NET 10 with WPF, delivered as a per-user
installer. The Python 1.x app it replaced was retired with 2.0.0. The plan of the rewrite is in
[docs/csharp-rewrite.md](docs/csharp-rewrite.md), and the decisions are in [docs/adr/](docs/adr/).

## Context

Deadlock has no FOV slider, so the fix is an `r_aspectratio` entry in the `ConVars` block of the
game's `gameinfo.gi`. Game updates overwrite that file. The app remembers the chosen value, plus any
extra config the user pasted, and puts it back.

Two things hold state:

- `gameinfo.gi` (and optionally `cfg/video.txt`) in the Steam library. The game reads these, and they
  are the only place a change has any effect.
- `%APPDATA%\DL-FOV-Fixer\config.json`. This is the user's remembered intent: the target value, the
  stored tweaks, and the app's own settings. It keeps 1.x's format (ADR 0004).

There is no server, no account and no network traffic except the update check.

## Projects

```text
DlFovFixer.App             net10.0-windows  WPF tray shell, view models, the only composition root
   |          \
   v           v
DlFovFixer.Infrastructure  net10.0-windows  files, registry, Steam, HTTP, manifest signatures
   |
   v
DlFovFixer.Core            net10.0          domain and use cases, no UI, files, registry or network
```

Dependencies point inward. Architecture tests keep Core free of the other layers, WPF, the registry
and the network, and keep Infrastructure free of the shell. `Directory.Build.props` reads the version
from `version.properties` and marks every build `-dev` unless it is built with
`-p:DlFovFixerReleaseBuild=true`.

## Core

- `GameInfo`: the merge as pure text functions (ADR 0005). `KeyValuesText` finds blocks with quotes
  and `//` comments honored, `BlockMerge` merges keys into one block and touches only the block's
  own keys, never a sub-block, `GameInfoMerge` applies the FOV and the tweaks together,
  `VideoConfigMerge` handles `cfg/video.txt`, `TweakTextParser` sorts a pasted config by key shape,
  and `AspectRatio` checks typed values. The vectors in `contracts/vectors/` define all of it.
- `Applying`: `ApplyService` reads through `IGameFiles`, merges, and writes only a file whose text
  changed. `StatusProbe` reads the file to find one of six `FixState` values, and after an apply it
  keeps a lock or a failure until the next apply. A file the game holds open is `Waiting`, not
  `Failed`.
- `Settings`, `Locating`, `Startup`: the ports `ISettingsStore`, `IGameInfoLocator` and
  `ISignInStartup`, and the settings record with its theme mode.
- `Updates`: `UpdateService` fetches `manifest.json` and `manifest.sig` from the latest release,
  checks the signature against the key built in from `contracts/keys/` before reading anything
  (ADR 0006), applies the version policy, downloads the installer and accepts it only when its size
  and SHA-256 match the signed manifest, then launches it.

## Infrastructure

- `FileSystemGameFiles`: UTF-8 without adding a byte-order mark, the one-time backup before the
  first change, and a write through a temporary file and `File.Replace`.
- `GameInfoWatcher`: a `FileSystemWatcher` on the stored `gameinfo.gi` that fires once the file has
  been quiet for three seconds.
- `JsonSettingsStore`: the 1.x `config.json`, read as forgivingly as 1.x read it, plus
  `schemaVersion` and `theme` on save. A file written by 1.0.0 is its test fixture.
- `SteamGameInfoLocator`: the registry, the common Steam folders and `libraryfolders.vdf`, behind
  `IRegistryReader` so tests never touch the real registry.
- `WindowsSignInStartup`: the `DL-FOV-Fixer` Run value, behind `IRunValues`.
- `Updates`: `GitHubReleaseChannel` (size limits on every read), `EcdsaSignatureVerifier` and
  `InstallerLauncher`.

## App

- `ViewModels/TrayViewModel` holds everything the menu does, behind the ports `IUserPrompts`,
  `INotifier` and `IFileOpener`, so it is tested without WPF. `UpdatesViewModel` runs the update
  check and install.
- The tray kit is the shared `DotNetLib.Tray` package from `dotnetlib` (see CONTRIBUTING.md for the
  feed). It brings the notification area icon (`TrayIconHost`), the menu builder, WPF UI's themes,
  the theme applier, the dialogs and the single-instance lock. The app keeps only what is its own.
- `Shell/TrayMenu` rebuilds the menu from the view models on every change with `TrayMenuBuilder`,
  `Shell/TrayNotifier` puts `INotifier` on the tray's balloons, and `Shell/StatusIconFactory` draws
  the vision-cone icon in the status color and has `IconFile` make the multi-size `.ico`.
- `Theming/WarmPalettes` are the app's light and dark `TrayPalette`s, given to `TrayThemeApplier` in
  place of the kit's neutral ones. Views read the kit's `Tray.*` brush keys.
- `App.xaml.cs` takes the single-instance lock, then merges the kit's resources
  (`TrayResources.Merge`). `Startup/StartupOptions` reads `--settings`, and `Composition/AppGraph`
  is the only composition root. It maps the saved `ThemeMode` to the kit's `TrayThemeMode`.

## Data flow

1. Start: load `config.json`. On the very first run, locate `gameinfo.gi` and keep the value already
   in it.
2. Apply: merge the FOV plus, when enabled, the stored tweaks, writing at most once per file and only
   if something changed. The first write to each file copies it to `<name>.dlfovfixer.bak`.
3. Watch: the watcher re-applies three seconds after a game update stops writing the file, and a
   timer re-applies every `periodic_check_minutes` as the fallback.
4. Status: the icon is green when the file holds the target value, amber when the value is absent,
   different or the file is locked, and red when the file cannot be found or read.

## Delivery

`installer/build-installer.ps1` publishes a self-contained build (a plain folder, not a single-file
bundle, ADR 0002), compiles the per-user Inno Setup installer, and writes its SHA-256,
`manifest.json` and `manifest.sig`. `.github/workflows/release-windows.yml` does that on a `v*` tag
and creates a draft release. The installer closes a running copy through the Restart Manager, starts
it again after an update, and on uninstall stops it and leaves `%APPDATA%` alone. The manifest is
signed with the key on the owner's offline drive and in the `DLFOVFIXER_MANIFEST_SIGNING_KEY`
secret. There is no code-signing certificate, so SmartScreen warns once on a browser download.

`.github/workflows/ci.yml` checks formatting, builds with warnings as errors, runs the tests with TRX
results uploaded even on failure, and parse-checks every PowerShell script.

## Known constraints

- **The game holds the file open while it runs.** Writes fail with a sharing violation, which is a
  normal transient state (`Waiting`) rather than an error.
- **Windows only**, by construction: the registry for Steam discovery and for the `Run` value, WPF
  for the shell.
- **The installer is not code-signed.** SmartScreen asks once per downloaded version. Updates from
  inside the app are not affected, because they are trusted through the signed manifest and carry no
  mark-of-the-web.
- **Deadlock may one day block matchmaking for changed ConVars.** Nothing confirms it yet. It is on
  the board to watch.
