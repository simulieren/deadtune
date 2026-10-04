using System.Windows.Controls;
using DlFovFixer.App.Shell;
using DlFovFixer.App.Tests.Support;
using DlFovFixer.App.ViewModels;
using DlFovFixer.Core.Applying;
using DlFovFixer.Core.Settings;
using static DlFovFixer.App.Tests.Support.GameInfoSamples;

namespace DlFovFixer.App.Tests.Shell;

public sealed class TrayMenuTests
{
    [Fact]
    public void Build_ShowsUnderscoresAsWritten()
    {
        var headers = Sta.Run(() => Items(Build()).Select(item => (string)item.Header).ToList());

        // A plain "_" is an access key marker in a menu header, so it must be doubled.
        Assert.Contains("Target: r__aspectratio 2.49 (~101°)", headers);
    }

    [Fact]
    public void Build_TogglesAreCheckableAndShowTheirState()
    {
        var toggles = Sta.Run(() => Items(Build())
            .Where(item => item.IsCheckable)
            .ToDictionary(item => (string)item.Header, item => item.IsChecked));

        Assert.True(toggles["Apply automatically on start"]);
        Assert.True(toggles["Check updates on start"]);
        Assert.False(toggles["Start with Windows"]);
        Assert.True(toggles["Same as Windows"]);
        Assert.False(toggles["Dark"]);
        Assert.True(toggles["100°   (r__aspectratio 2.49)"]);
    }

    [Fact]
    public void Build_PlainActionsAreNotCheckable()
    {
        var plain = Sta.Run(() => Items(Build()).Where(item => !item.IsCheckable).Select(item => (string)item.Header).ToList());

        Assert.Contains("Apply now", plain);
        Assert.Contains("Quit", plain);
    }

    private static ContextMenu Build()
    {
        var files = new FakeGameFiles();
        files.Texts[GameInfoPath] = GameInfoWith249;
        var shell = new FakeShell();
        var store = new FakeSettingsStore(AppSettings.Defaults with { GameInfoPath = GameInfoPath, FovValue = "2.49" });
        var model = new TrayViewModel(store, new ApplyService(files), new StatusProbe(files), new FakeLocator(), shell, files, shell, shell, shell);
        var updates = new UpdatesViewModel(null!, "2.0.0", "https://example.invalid", shell, shell, shell);
        return TrayMenu.Build(model, updates, () => { });
    }

    private static IEnumerable<MenuItem> Items(ItemsControl parent)
    {
        foreach (var item in parent.Items.OfType<MenuItem>())
        {
            yield return item;
            foreach (var child in Items(item))
            {
                yield return child;
            }
        }
    }
}
