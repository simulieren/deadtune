# Pitfalls

Mistakes this repository has already made, kept short so they are not made twice. When something
fails in a way you did not expect, search here for the error text before debugging it.

Add an entry in the same commit as the fix when a bug took longer to find than to fix, came back, or
came from a tool or platform trap. Put new entries at the top, in this shape:

```markdown
## What goes wrong, in a few words

- Symptom: the exact error text, or what you see
- Cause: why it happens
- Fix: what to do about it
- Closed off by: the test, check, or tool that now catches it, or "not yet"
- Seen: date and where (milestone, PR, or commit)
```

A pitfall that could hit another repository is also reported to CodePrint. CodePrint's
`docs/pitfalls/README.md` explains how.

## Uninstall leaves the running app and its files behind

- Symptom: after an uninstall with the app in the tray, the app keeps running, the uninstall entry
  and shortcut are gone, and the log is full of:

  ```text
  Failed to delete the file; it may be in use (5).
  ```

- Cause: `CloseApplications` makes Setup close the app through the Restart Manager, but the
  uninstaller does not. Then a first fix, an `[UninstallRun]` PowerShell step, failed with exit
  code 1: Inno Setup turns `{{` into `{` in a parameter, but leaves `}}` as it is, so the script
  block ended in `}}`.
- Fix: `[UninstallRun]` stops only the copy running from `{app}` and waits for it, with `{{` for an
  opening brace and a plain `}` for a closing one. The uninstall log shows the step's exit code.
- Closed off by: not yet, only the hand test in the installer checklist. Check `Process exit code: 0`
  and `Failed to delete` in an uninstall log (`unins000.exe /SILENT /LOG=<file>`).
- Seen: 2026-09-30, the first hand test of the 2.x installer, before any release.

## A test that creates a WPF Application breaks WPF tests running beside it

- Symptom: a WPF test fails now and then, about one run in four:

  ```text
  System.InvalidOperationException : Cannot access Freezable 'System.Windows.Media.SolidColorBrush' across threads because it cannot be frozen.
  ```

  thrown while a `MenuItem` is created, in `TrayMenuTests`.
- Cause: xUnit runs test classes in parallel, each on its own STA thread here. While one test holds
  a WPF `Application` with WPF UI's resources, every element made on another thread takes its
  implicit styles, and their brushes belong to the Application's thread.
- Fix: put a test that creates an `Application` in the `WpfApplicationCollection`, which turns off
  parallel runs, so xUnit runs it on its own after the other tests.
- Closed off by: `WpfApplicationCollection` on `ThemeResourcesTests`. 15 full runs in a row passed
  after the fix.
- Seen: 2026-09-30, on `main` right after the dark theme (#10) merged. CI had passed.

## WPF UI's Window style makes every plain Window throw when it is shown

- Symptom:

  ```text
  System.InvalidOperationException: Cannot change AllowsTransparency after a Window has been shown or WindowInteropHelper.EnsureHandle has been called.
  ```

  from `Window.Show()` or `ShowDialog()` on any `new Window { ... }` once WPF UI's
  `ControlsDictionary` is merged into the application resources.
- Cause: `ControlsDictionary` brings an implicit style for `Window`, made for WPF UI's
  `FluentWindow`, that sets `AllowsTransparency`. A window made in code is only initialized when it
  is shown, so the style lands after the window handle exists, and WPF refuses the change.
- Fix: `Theming/Theme.xaml`, merged after WPF UI's dictionaries, defines its own implicit `Window`
  style that only sets the theme's background and foreground. Since the move to `DotNetLib.Tray`,
  that style is the kit's `Themes/Tray.xaml`, which `TrayResources.Merge` adds last.
- Closed off by: `ThemeResourcesTests.WindowMadeInCode_OpensInBothThemesWithTheWarmPalette`, which
  failed with the message above when that style was removed (checked before the move), and the
  kit's own `TrayResourcesTests`.
- Seen: 2026-09-30, adding the dark theme, in an off-screen render before it shipped.

## Creating the App class in a tool or a test starts the real app

- Symptom: a harness that only meant to render the menu wrote the user's real `gameinfo.gi` and
  switched the theme it was testing.
- Cause: `new DlFovFixer.App.App()` queues WPF's startup on the dispatcher. The first time the
  dispatcher runs, `App.OnStartup` builds the whole graph with the real `config.json`, applies the
  fix and starts the timers.
- Fix: never create the App class outside the app. Build a plain `Application` and call
  `TrayResources.Merge` on its resources, as `App.xaml.cs` does, or run the app with
  `--settings <path>` on a scratch config.
- Closed off by: `ThemeResourcesTests` loads the resources on a plain `Application`, and its summary
  says why. No automatic check stops a new tool from making the mistake.
- Seen: 2026-09-30, the dark theme work. The merge found the file already correct, so its content
  did not change.

## A line break typed as \n through a tool lands inside a Python string

- Symptom:

  ```text
  SyntaxError: unterminated string literal (detected at line 119)
  ```

  when the app starts, while `py -m pytest -q` passes.
- Cause: an edit made through a shell heredoc or a script turned each `\n` escape in a new
  f-string into a real line break. No test imports `dlfovfixer/app.py`, because it needs pystray and
  a display, so nothing compiled the broken file.
- Fix: write the escapes again, and check a changed Python file with `py -m py_compile` before
  committing it.
- Closed off by: `tests/test_compile.py`, which compiled every module in `dlfovfixer/` until the
  Python app was removed in 2.0.0. The trap itself still applies to any file edited through a shell.
- Seen: 2026-09-30, the 1.0.2 updater bridge, in review before it was merged.

## Replacing a file the game holds open says "access denied", not "in use"

- Symptom:

  ```text
  System.UnauthorizedAccessException: Access to the path '...\gameinfo.gi' is denied.
  ```

  from `File.Move(temp, path, overwrite: true)` while Deadlock is running. It maps to a red Failed
  state instead of Waiting, although nothing is wrong except that the game has the file open.
- Cause: moving over a file that another process has open fails with Win32 error 5, access denied,
  whatever share mode that process used. `File.Replace` on the same file fails with error 32, the
  sharing violation that means Waiting. Measured on Windows 11 with .NET 10: with the file open for
  reading and `FileShare.Read`, `File.Move` gives 5 and `File.Replace` gives 32.
- Fix: replace an existing file with `File.Replace(temp, path, null)`, and use `File.Move` only when
  the target does not exist yet.
- Closed off by: `FileSystemGameFilesTests.WriteText_FileOpenInTheGame_IsLockedAndLeavesTheFileAlone`
  with `FileShare.Read`, which fails if `File.Replace` is swapped back for `File.Move`.
- Seen: 2026-09-29, M4, while writing `FileSystemGameFiles`, before it shipped.

## A PyInstaller one-file build is reported as a trojan

- Symptom: Windows Defender reports `DL-FOV-Fixer.exe` as a trojan and removes it, on the build
  machine and for anyone who downloads the release.
- Cause: the one-file bootloader unpacks the interpreter into a temporary folder and runs it from
  there, which is the shape most droppers have, so the match is on the packaging and not on anything
  the app does. `build.bat` does not pass `--noupx`, so a build machine with `upx` on PATH compresses
  the executable too and makes the match stronger.
- Fix: sign the artifact and let it build reputation, submit it to Microsoft as a false positive, and
  stop shipping a self-extracting bundle. The last one is the C# rewrite
  ([docs/csharp-rewrite.md](csharp-rewrite.md)). `--onedir` and a self-built bootloader are the cheap
  experiments if the Python build has to live longer.
- Closed off by: not yet. The release workflow's signing step and its step summary at least make an
  unsigned release visible in the run.
- Seen: 2026-08 onwards, v1.0.0.

## A plain `$` in a regex does not match at the end of a CRLF line

- Symptom: an unquoted `SceneSystem` key is found but never updated, and only in files with Windows
  line endings. The same case passes on a file with LF endings.
- Cause: in Python's `re`, `$` under `MULTILINE` matches only right before `\n`. On CRLF text the
  `\r` sits between the value and the `\n`, and `\r` counts as whitespace, so `(\S+)[ \t]*$` stops
  before the `\r` and then cannot match.
- Fix: end the pattern with a line-break lookahead, `(?=\r?\n|\Z)`, instead of `$`. In .NET write
  `\z`, because .NET's `\Z` also matches before a final `\n`.
- Closed off by: the CRLF form of every case in `contracts/vectors/gameinfo-merge.json` and
  `gameinfo-apply.json`, run by `DlFovFixer.Core.Tests` (and by the Python tests until 2.0.0). Removing the
  `\r?` from the C# lookahead fails four of them.
- Seen: 2026-09, in `gameinfo._merge_one`.

## A locked gameinfo.gi is a normal state, not an error

- Symptom:

  ```text
  PermissionError: [WinError 32] The process cannot access the file because it is being used by
  another process
  ```

  The tray icon turns red while Deadlock is running, and goes back to normal after the game closes.
- Cause: the game, or a mod manager, holds `gameinfo.gi` or `cfg/video.txt` open. A write then fails
  with a sharing violation, Win32 error 32 or 33, which is expected and heals itself on the next pass.
- Fix: treat those two error numbers as a transient waiting state. Keep the last known status, retry
  on the next tick, and say "file is in use (is Deadlock running?)" when the user asked for the apply
  explicitly. Any other permission error is still a real error.
- Closed off by: in the C# port, `FileSystemGameFiles` reports errors 32 and 33 as a locked file and
  `ApplyService` turns that into `ApplyResult.Waiting`. `FileSystemGameFilesTests` opens the file with
  `FileShare.None` and `FileShare.Read`, and `ApplyServiceTests.Apply_GameHasTheFileOpen_IsWaiting`
  covers the mapping. The Python app still handles it by hand in `app.py`.
- Seen: 2026-09, commit 887fe47.

## The `python` command writes to a private copy of AppData

- Symptom: a script saves a file under `%APPDATA%` and reads it back fine, but the real app,
  PowerShell and `py` still see the old file, or none.
- Cause: `python` can resolve to the Microsoft Store build through its WindowsApps alias. Started from
  an agent tool inside a packaged app, it runs under that package's file-system virtualization, so
  writes land in `%LOCALAPPDATA%\Packages\<package>\LocalCache` instead of the real folder.
- Fix: use `py`, the launcher of a normal Python install, or PowerShell, for anything that must touch
  real user folders. `(Get-Command python).Source` under `WindowsApps` means it is the Store build.
- Closed off by: not yet.
- Seen: 2026-08. Also in CodePrint's `docs/pitfalls/windows-tooling.md`.
