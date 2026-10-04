using System.Windows;
using DotNetLib.Tray;
using Microsoft.Win32;

namespace DlFovFixer.App.Shell;

/// <summary>
/// The dialogs, in WPF and in the app's theme: <paramref name="prepare"/> gives each window the
/// theme's colors. The file picker is Windows' own and follows Windows' theme. A tray app has no
/// window to own it, so it gets a hidden topmost owner and cannot open behind the game.
/// </summary>
public sealed class WpfUserPrompts(string title, Action<Window> prepare) : IUserPrompts
{
    public string? AskValue(string current) => TrayTextWindow.AskLine(
        title,
        "Enter the r_aspectratio value. Higher is a wider FOV. Examples:\n" +
        "  1.75 ≈ 80°    2.15 ≈ 90°    2.49 ≈ 100°\n" +
        "  2.66 ≈ 105°   2.83 ≈ 110°   3.00 ≈ 115°",
        current,
        prepare);

    public string? AskGameInfoPath()
    {
        var dialog = new OpenFileDialog
        {
            Title = "Locate Deadlock's gameinfo.gi",
            Filter = "Deadlock game info|gameinfo.gi|All files|*.*",
        };
        return WithOwner(owner => dialog.ShowDialog(owner) == true ? dialog.FileName : null);
    }

    public string? AskPastedConfig() => TrayTextWindow.AskLines(
        "Import Deadlock config",
        "Paste a Deadlock config below (ConVars, SceneSystem or video.cfg settings). Keys are sorted by themselves:\n" +
        "  setting.* goes to video.cfg, PascalCase to SceneSystem, and the rest to ConVars.\n" +
        "r_aspectratio sets your FOV. Comments and headers are ignored.",
        prepare);

    public bool Confirm(string message) => TrayMessageWindow.Confirm(title, message, prepare);

    public void Inform(string message) => TrayMessageWindow.Inform(title, message, prepare);

    public void ShowText(string heading, string text) => TrayTextWindow.Show($"{title}: {heading}", text, prepare);

    private static T WithOwner<T>(Func<Window, T> show)
    {
        var owner = new Window
        {
            Topmost = true,
            ShowInTaskbar = false,
            WindowStyle = WindowStyle.None,
            ResizeMode = ResizeMode.NoResize,
            Width = 0,
            Height = 0,
            Left = SystemParameters.WorkArea.Width / 2,
            Top = SystemParameters.WorkArea.Height / 2,
        };
        owner.Show();
        owner.Activate();
        try
        {
            return show(owner);
        }
        finally
        {
            owner.Close();
        }
    }
}
