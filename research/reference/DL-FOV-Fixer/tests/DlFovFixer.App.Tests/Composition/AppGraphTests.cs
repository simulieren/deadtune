using DlFovFixer.App.Composition;
using DlFovFixer.Core.Settings;
using DotNetLib.Tray;

namespace DlFovFixer.App.Tests.Composition;

/// <summary>
/// Only the pure parts of the graph. Building it would start the real tray, settings and watcher.
/// </summary>
public sealed class AppGraphTests
{
    [Theory]
    [InlineData(ThemeMode.System, TrayThemeMode.System)]
    [InlineData(ThemeMode.Light, TrayThemeMode.Light)]
    [InlineData(ThemeMode.Dark, TrayThemeMode.Dark)]
    public void ToTrayMode_KeepsTheSavedChoice(ThemeMode saved, TrayThemeMode expected) =>
        Assert.Equal(expected, AppGraph.ToTrayMode(saved));
}
