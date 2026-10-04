using DotNetLib.Tray;

namespace DlFovFixer.App.Shell;

/// <summary>Speaks through the tray icon's balloons, under the app's title.</summary>
public sealed class TrayNotifier(TrayIconHost tray, string title) : INotifier
{
    public void Notify(string message) => tray.Notify(title, message);
}
