using System.Windows.Controls;
using DlFovFixer.App.ViewModels;
using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Settings;
using DotNetLib.Tray;

namespace DlFovFixer.App.Shell;

/// <summary>
/// Builds the tray menu from the view model, with the items and order 1.0 had. It is built again
/// whenever the view model changes, so every label and check mark is current when it opens.
/// <see cref="TrayMenuBuilder"/> escapes "_" in headers and makes toggles checkable.
/// </summary>
public static class TrayMenu
{
    public static ContextMenu Build(TrayViewModel model, UpdatesViewModel updates, Action quit)
    {
        var settings = model.Settings;
        return new TrayMenuBuilder()
            .Label(model.StatusLine)
            .Label(model.TargetLine)
            .Separator()
            .Item("Apply now", model.ApplyNow, isDefault: true)
            .Item("Check file now", model.CheckNow)
            .Submenu("Set FOV value", presets =>
            {
                foreach (var preset in FovPresets.All)
                {
                    var value = preset.Value;
                    presets.Item($"{preset.Degrees}°   (r_aspectratio {value})", () => model.SetValue(value), isChecked: settings.FovValue == value);
                }

                presets.Separator();
                presets.Item("Custom value…", model.ChooseCustomValue);
            })
            .Submenu("Extra tweaks", tweaks => tweaks
                .Label(model.StoredTweaksLine)
                .Separator()
                .Item("Paste / import config…", model.ImportTweaks)
                .Item("View stored tweaks…", model.ViewTweaks)
                .Item("Clear stored tweaks…", model.ClearTweaks)
                .Separator()
                .Item("Apply extra tweaks (not just FOV)", model.ToggleApplyTweaks, isChecked: settings.ApplyTweaks))
            .Separator()
            .Item(updates.CheckLabel, () => _ = updates.CheckAsync(interactive: true), isEnabled: !updates.IsChecking)
            .Item(updates.InstallLabel, () => _ = updates.InstallAsync(), isEnabled: updates.CanInstall)
            .Item("Check updates on start", model.ToggleCheckUpdatesOnStart, isChecked: settings.CheckUpdatesOnStart)
            .Separator()
            .Item("Open gameinfo.gi", model.OpenGameInfo)
            .Item("Locate gameinfo.gi…", model.LocateGameInfo)
            .Separator()
            .Item("Apply automatically on start", model.ToggleAutoApply, isChecked: settings.AutoApplyOnStart)
            .Item("Start with Windows", model.ToggleSignInStartup, isChecked: model.IsSignInStartupEnabled)
            .Submenu("Theme", theme =>
            {
                foreach (var (mode, label) in new[] { (ThemeMode.System, "Same as Windows"), (ThemeMode.Light, "Light"), (ThemeMode.Dark, "Dark") })
                {
                    theme.Item(label, () => model.SetTheme(mode), isChecked: settings.Theme == mode);
                }
            })
            .Separator()
            .Item("About", model.ShowAbout)
            .Item("Quit", quit)
            .Build();
    }
}
