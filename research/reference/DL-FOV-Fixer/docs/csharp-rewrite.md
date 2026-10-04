# Plan: rewrite DL-FOV-Fixer in C#

Status: **done.** 2.0.0 shipped on 2026-09-30 and the Python app was removed in M7. This file is
kept as the record of the plan; [ARCHITECTURE.md](../ARCHITECTURE.md) describes the app as it is.
Updates are trusted through a signed manifest instead of a code-signing certificate (ADR 0006). M1
found no detection on the 2.0.0 installer (see "M1 results" below).

Where the C# app differs from 1.0 on purpose:

- "View stored tweaks" shows the list in a read-only window instead of writing a temporary file.
- Messages use a colon where 1.0 used a dash, as this repository's writing rules ask.
- A second launch tells the running copy, which says it is already running. 1.0 started a second
  tray icon.
- The menu and the dialogs follow Windows' light or dark setting, or a theme chosen in the menu.
  1.0 had no theme support. Messages use the app's own window instead of the Win32 message box,
  which cannot be themed.
- A build that cannot update itself, a dev build or one without the manifest key, offers the
  releases page from "Check for updates" instead.

## Why

Windows Defender reports the released `DL-FOV-Fixer.exe` as a trojan. The file is a PyInstaller
one-file build, and its bootloader is exactly what heuristics match: a self-extracting stub that
unpacks code into a temporary folder and runs it. The same stub ships in a large share of real
malware, so the pattern is what gets matched, not anything this app does. `build.bat` also never
passes `--noupx`, so any build machine that happens to have `upx` on PATH compresses the executable
as well, which makes the match stronger. Nothing in the app's own behavior causes the verdict, so no
amount of source cleanup removes it.

Two fixes exist, and they are independent:

1. Sign the artifact and let it build reputation. That work is tracked under the **Trusted release**
   milestone, and it applies to the Python build as it stands today.
2. Stop shipping a self-extracting interpreter bundle. That is this rewrite.

The second fix is the one that also pays for itself elsewhere: a .NET tray app starts faster, the
whole config-file surface becomes typed and testable, and the repository moves onto the CodePrint
.NET profile that GoalMaker already proves.

The rewrite is not justified by the detection alone. If signing clears the verdict first, the
rewrite still stands on its own, but it stops being urgent. Milestone M1 below measures that before
any logic is ported.

## What must not change

These are promises the current app already makes, and the port keeps every one of them:

- Only the keys the user chose are written. The rest of `gameinfo.gi` survives byte for byte,
  newlines included.
- A key that already exists as a nested sub-block (`rate`, `speaker_config`) is never rewritten.
  Mangling those stops Deadlock from launching.
- A one-time backup (`gameinfo.gi.dlfovfixer.bak`) is made before the first change to a file, and an
  existing backup is never overwritten.
- Applying is idempotent. An already-correct file is not written at all.
- The chosen FOV and the pasted extra tweaks survive a game update, a reboot, and an app update.
- Settings stay in `%APPDATA%\DL-FOV-Fixer\config.json`, so an existing install carries over.

## Decisions

| ADR | Decision |
|---|---|
| [0001](adr/0001-rewrite-the-tray-app-in-csharp.md) | C# on .NET 10, WPF shell, Core plus Infrastructure plus App |
| [0002](adr/0002-a-per-user-installer-replaces-the-portable-exe.md) | Signed per-user installer, self-contained, replaces the portable exe |
| [0003](adr/0003-updates-through-a-release-manifest-and-a-publisher-check.md) | Update channel is a release manifest plus an Authenticode publisher check (publisher check replaced by 0006) |
| [0006](adr/0006-trust-updates-through-a-signed-manifest-not-a-certificate.md) | Updates are trusted through a signed manifest, not a code-signing certificate |
| [0004](adr/0004-keep-the-config-json-contract.md) | The 1.0 `config.json` stays the settings contract |
| [0005](adr/0005-keep-the-surgical-text-merge.md) | Keep surgical text merging, not a KeyValues round trip |

## Target layout

Windows only, so the solution sits at the repository root rather than under a platform folder.

```text
DL-FOV-Fixer.slnx
Directory.Build.props          one version for every project, "-dev" unless it is a release build
global.json                    pinned SDK, Microsoft.Testing.Platform runner
version.properties             versionName=2.0.0
src/
  DlFovFixer.Core/             net10.0. Domain and use cases. No UI, no registry, no file system.
  DlFovFixer.Infrastructure/   net10.0-windows. File system, registry, Steam, HTTP, manifest signatures.
  DlFovFixer.App/              net10.0-windows. WPF tray shell and the composition root.
tests/
  DlFovFixer.Core.Tests/
  DlFovFixer.Infrastructure.Tests/
  DlFovFixer.App.Tests/
contracts/vectors/             behavior vectors read by both the Python and the C# tests
installer/
  DL-FOV-Fixer.iss             per-user Inno Setup definition
  build-installer.ps1          publish, compile the installer, write the SHA-256 beside it
tools/                         the signing scripts that already exist
```

Dependencies point inward: `App` and `Infrastructure` depend on `Core`, and `Core` depends on
nothing. `App` owns the only composition root (`Composition/AppGraph.cs`).

## Port map

| Today (Python) | Becomes | Project |
|---|---|---|
| `gameinfo.py` brace matching, block spans, comment and quote handling | `GameInfo/KeyValuesText.cs`, `BlockSpan.cs` | Core |
| `gameinfo.py` `merge_block`, `_merge_one` | `GameInfo/BlockMerge.cs`, `MergeAction.cs`, `KeyMerge.cs`, `BlockMergeOutcome.cs` | Core |
| `gameinfo.py` `normalize_value`, `aspect_to_fov`, `PRESETS` | `GameInfo/AspectRatio.cs`, `FovPreset.cs`, `FovPresets.cs` | Core |
| `gameinfo.py` `merge_video_cfg` | `GameInfo/VideoConfigMerge.cs`, `VideoConfigOutcome.cs` | Core |
| `gameinfo.py` `apply_config`, `read_current` | `GameInfo/GameInfoMerge.cs`, `GameInfoMergeOutcome.cs` for the text, `Applying/ApplyService.cs` for reading and writing (M4) | Core |
| `app.py` `_compute_status` | `Applying/StatusProbe.cs`, `FixState.cs`, `FixStatus.cs` | Core |
| `tweaks.py` `parse`, `_route`, `merge_lists` | `GameInfo/TweakTextParser.cs`, `ParsedTweaks.cs`, `TweakEntry.cs` | Core |
| `updater.py` version compare and asset choice | `Updates/SemanticVersion.cs`, `UpdatePolicy.cs`, `ReleaseManifest.cs`, `ReleaseManifestParser.cs`, `ReleaseChannelAddress.cs`, `UpdateService.cs` | Core |
| `gameinfo.py` `_read`, `_write`, `_ensure_backup`, `restore_backup`, `video_settings_path` | `GameFiles/FileSystemGameFiles.cs` behind `Core/Applying/IGameFiles.cs` | Infrastructure |
| `locator.py` | `Locating/SteamGameInfoLocator.cs`, `SteamLibraries.cs` behind `Core/Locating/IGameInfoLocator.cs` | Infrastructure |
| `config.py` | `Settings/JsonSettingsStore.cs`, `SettingsDocument.cs` behind `Core/Settings/ISettingsStore.cs` | Infrastructure |
| `startup.py` | `Startup/WindowsSignInStartup.cs` behind `Core/Startup/ISignInStartup.cs` | Infrastructure |
| `updater.py` download and install | `Updates/GitHubReleaseChannel.cs`, `EcdsaSignatureVerifier.cs`, `InstallerLauncher.cs` | Infrastructure |
| `iconfactory.py` | `Shell/StatusIconFactory.cs` | App |
| `app.py` tray icon, menu, dialogs, timer | `Shell/`, `Views/`, `ViewModels/`, `Startup/` | App |
| `run.pyw`, `__main__.py` | `App.xaml.cs` plus `Composition/AppGraph.cs` | App |

`patch()` and `fov_to_aspect()` are not ported. Only the Python tests call them: the app applies
through `apply_config()` and never converts degrees back to a value.

The shapes for the update seam, the tray icon, single instance and the Run key follow GoalMaker's
`windows/src` (inspected 2026-09-28). They are reimplemented here, not copied.

## Behavior vectors instead of a rewritten test suite

The merge logic is the only part of this app that can break someone's game, so it does not get a
fresh set of hand-written C# tests. The Python test cases became data in `contracts/vectors/`, and
both implementations read the same files ([contracts/vectors/README.md](../contracts/vectors/README.md)):

```text
contracts/vectors/
  gameinfo-merge.json          multi-key merges, including nested sub-blocks and comments
  gameinfo-apply.json          the value and the tweaks applied to a whole file
  video-config.json            create and merge cases for cfg/video.txt
  tweak-parsing.json           pasted blobs and the routed, de-duplicated result
  fov-value.json               accepted values, degrees and the presets
```

The update slice (M6) adds `semantic-version.json` for version compare and the offer policy, and
`release-channel.json` for which artifact paths are allowed and which must be refused.

Cases the current Python tests do not cover, added while writing the vectors because they are the
ways this code has actually gone wrong before:

- CRLF and LF variants of every merge case. A bare `$` in a regex already broke the unquoted
  SceneSystem path once on CRLF files.
- A `ConVars` block holding both a nested `rate` sub-block and the key being merged.
- A file whose only closing brace is the root one, and a file with no root block at all.
- A value with a trailing `//` comment on the same line, which must survive an update in place.
- A second apply straight after the first, asserting that no write happened.
- Typed values that Python's `float()` takes and .NET does not, like `0_6` and full-width digits.
  Both now accept only a plain ASCII number, so neither can write such a value into the file.

The Python tests keep running from these vectors until `dlfovfixer/` is removed, which is what makes
the port verifiable rather than hopeful.

## Status model

Today the tray has three colors and the code has three states, so a file locked by the running game
has to be squeezed into one of them. The port keeps the three colors and names five states behind
them:

| State | Meaning | Icon |
|---|---|---|
| `Ok` | The target value is in the file | Green |
| `NotApplied` | File found, the key is absent | Amber |
| `Drifted` | File found, the value differs, usually after a game update | Amber |
| `Waiting` | The file is locked (Win32 error 32 or 33), the game is probably running | Amber |
| `Missing` | No `gameinfo.gi` at the stored path, and none found | Red |
| `Failed` | Read or write failed for any other reason | Red |

`Waiting` is the state commit 887fe47 added to the Python app as a special case. Making it a real
state is what stops a retry loop from painting the icon red while Deadlock is running.

## Settings and coming from 1.0.x

`%APPDATA%\DL-FOV-Fixer\config.json` keeps its path and its keys, and the reader stays tolerant:
unknown keys are ignored, missing keys fall back to defaults, and a corrupt file is replaced by
defaults rather than refused. A `schemaVersion` is written from 2.0.0 on, and its absence means
version 1.

The Run key value keeps its name, `DL-FOV-Fixer`, under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. An installed 1.0.x copy points that value at
wherever the portable exe sat, so on first run the app rewrites the value to the installed
executable when the setting is on, and clears it when it points at a file that no longer exists.

One trap needs handling before 2.0.0 is published. The 1.0.0 updater accepts **any** `.exe` asset,
preferring names that contain `dl-fov-fixer` and then the alphabetically first one. Both parts of
`DL-FOV-Fixer-2.0.0-setup.exe` match, and `-` sorts before `.`, so an old copy would pick the
installer, copy it over its own exe, and leave a Run value that starts the installer at every
sign-in. The plan is therefore a small bridge release first:

1. Publish Python **1.0.2**, whose only change is that the updater requires an asset named exactly
   `DL-FOV-Fixer.exe`. Without one it notifies and opens the releases page instead of installing.
2. Publish **2.0.0** with the installer only. A 1.0.2 copy offers the page. A copy that never took
   the bridge is handled by the release notes and by the first-run Run-key repair above.

## Updates and delivery

Delivery becomes a signed per-user Inno Setup installer built from a deterministic self-contained
publish, which is the CodePrint .NET profile and the shape GoalMaker ships. The AppId is fixed at
the first release and never changes. `CloseApplications` plus a filter on the executable lets the
Restart Manager close the running tray app during an in-place update. Uninstall leaves
`%APPDATA%\DL-FOV-Fixer` alone, so a reinstall keeps the FOV and the stored tweaks.

Self-contained is deliberate: the app must work the moment it is installed, with no .NET runtime
prerequisite. Inside the installer the publish stays a normal folder rather than a single-file
bundle, because single-file extraction into a temporary folder is the very behavior that earned the
Python build its trojan verdict. A portable single-file asset can be added later if anyone asks for
one, and it would be a separate artifact with its own name.

The updater splits into the five stages CodePrint requires, each behind its own seam:

```text
release channel -> version policy -> artifact selector -> verified download -> installer launcher
```

`manifest.json` on the latest release carries the version and, per artifact, the file name, size and
SHA-256, and `manifest.sig` beside it is an ECDSA P-256 signature over its exact bytes. The app checks
that signature against the key it was built with before it reads the manifest, and the downloaded
installer must then match the signed size and hash before it is allowed to run (ADR 0006). Dev
builds never update themselves, pre-releases are never offered, and the releases page stays the
manual path. This is the scheme GoalMaker uses, and it replaced ADR 0003's Authenticode publisher
check because no certificate is bought.

## Tests

- **Core**: the vectors above, plus state transitions in `StatusProbe`, the update service driven by
  a fake channel, verifier and installer, and the refusal cases in `ReleaseChannelAddress`
  (`..`, absolute paths, a version other than the manifest's).
- **Infrastructure**: real files in a temp directory for byte-exact preservation, backup-once and
  atomic replace, and the locked-file mapping (open the file with `FileShare.None`, expect
  `Waiting`). Settings round trip, plus reading a checked-in 1.0.0 `config.json` fixture. The Steam
  locator against a fake registry reader and a fake `libraryfolders.vdf` tree, so no test touches
  the real registry.
- **App**: view models, `StartupOptions` parsing, single-instance forwarding, and the state to color
  mapping. STA where WPF needs it.
- **CI**: `dotnet format --verify-no-changes`, build with warnings as errors, tests with TRX
  uploaded even on failure, a PowerShell parse check over `installer/` and `tools/`, and CodePrint's
  `validate_repository.py`.

## Milestones

| # | Slice | Leaves behind |
|---|---|---|
| M0 | Design (this document and the ADRs) | Decisions recorded, board items created |
| M1 | Trust measurement | A signed throwaway WPF build, and Defender, SmartScreen and VirusTotal verdicts written down |
| M2 | Scaffold | Solution, three projects, versioning, `.editorconfig`, CI green, validator passing |
| M3 | Core port | Vectors extracted, both implementations passing them |
| M4 | Adapters | Files, settings, Steam locator, Run key, all tested |
| M5 | Tray shell | Menu parity, icon states, dialogs, theming, single instance |
| M6 | Updates and delivery | Update seam, installer, signed release workflow, the 1.0.2 bridge |
| M7 | Retire Python | `dlfovfixer/` removed, docs rewritten, 2.0.0 released |

M1 comes before the scaffold on purpose. It is a day of work that can retire the whole rewrite. It
was planned around a certificate that is now not being bought (ADR 0006), so it measures the
unsigned .NET installer instead.

### M1 results

Measured on 2026-09-30 on the unsigned installer from the v2.0.0 release,
`DL-FOV-Fixer-2.0.0-setup.exe`, SHA-256 `932b47a7a058ccae568d045bedcacbfbd2f85e8e03ed5c5d83a960413f0ca571`:

| Check | Result |
|---|---|
| Microsoft Defender custom scan of the installer (platform 4.18.26080.4, signatures 1.459.480.0, real-time protection on) | No threats found |
| Microsoft Defender custom scan of the installed app folder | No threats found |
| VirusTotal file scan of the installer ([report](https://www.virustotal.com/gui/file/932b47a7a058ccae568d045bedcacbfbd2f85e8e03ed5c5d83a960413f0ca571)) | 0 of 69 engines flag it |
| VirusTotal scan of the release download link | 0 of 92 URL scanners flag it |
| The 1.x PyInstaller `DL-FOV-Fixer.exe`, for comparison | Reported by Defender as a trojan, which is why the rewrite exists |

So the rewrite did what it was for: the self-contained .NET app in a per-user installer carries no
detection, without a code-signing certificate. Not measured: the SmartScreen prompt on a browser
download. It is expected, because the installer is unsigned and new, and it asks once per version
(README, "Install").

## Risks

- **The verdict may not move.** A self-contained .NET app is not automatically clean, and an
  unsigned one is not clean at all. M1 measures this before anything is ported. (It moved: see
  "M1 results".)
- **Size.** The publish folder is roughly 60 to 80 MB against today's 19 MB, and WPF cannot be
  trimmed. The installer compresses it, but the download grows.
- **The merge is the dangerous part.** It is the only code that writes into a file the game must be
  able to parse. Vectors first, backups kept, and no reformatting.
- **Two implementations at once.** M3 to M6 keep the Python app on `main`. Only M7 removes it, in
  one commit.

## Provenance

CodePrint rules and the .NET profile (`RULES.md`, `docs/architecture-and-modularity.md`,
`docs/delivery-and-updates.md`, `docs/testing-and-ci.md`, `templates/dotnet/`), and GoalMaker's
`windows/` app, both inspected 2026-09-28. CodePrint's UI guide points at `dotnetlib` for a reusable
updater seam and file-system abstractions. The current `dotnetlib` has neither, so the seams here are
written locally and stay small enough to move into `dotnetlib` if a second app needs them.
