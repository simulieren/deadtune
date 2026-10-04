# ADR 0001: Rewrite the tray app in C# on .NET 10

The released Python build is reported as a trojan because a PyInstaller one-file executable
compressed with UPX is shaped like a self-extracting dropper, and no change inside the app can alter
that. Signing is the cheaper fix and is being done anyway, so this rewrite is justified by what it
leaves behind rather than by the verdict alone: a typed, testable config-file layer, a tray app that
starts without unpacking an interpreter, and the CodePrint .NET profile that GoalMaker already runs
on. The shape is GoalMaker's: a pure `DlFovFixer.Core` on `net10.0` holding the domain and the use
cases, a `DlFovFixer.Infrastructure` on `net10.0-windows` for the file system, the registry, Steam
and HTTP, and a WPF `DlFovFixer.App` that owns the only composition root. WPF with
`H.NotifyIcon.Wpf` and `CommunityToolkit.Mvvm` is chosen over WinForms so the app gets the
light, dark and system theming CodePrint requires and can consume `dotnetlib` later, at the cost of
a larger self-contained publish, since WPF cannot be trimmed and cannot be compiled ahead of time.
