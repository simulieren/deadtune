using DlFovFixer.Core.GameInfo;

namespace DlFovFixer.Core.Settings;

/// <summary>
/// Everything the app remembers between runs. Stored in the 1.0 config.json format, so an existing
/// install keeps its settings (ADR 0004).
/// </summary>
public sealed record AppSettings(
    string GameInfoPath,
    string FovValue,
    bool AutoApplyOnStart,
    int PeriodicCheckMinutes,
    bool StartWithWindows,
    bool CheckUpdatesOnStart,
    ExtraTweaks Tweaks,
    bool ApplyTweaks,
    ThemeMode Theme = ThemeMode.System)
{
    /// <summary>
    /// The defaults, used for any setting that is missing or unreadable. All but <see cref="Theme"/>
    /// are 1.0's.
    /// </summary>
    public static AppSettings Defaults { get; } = new(
        GameInfoPath: string.Empty,
        FovValue: AspectRatio.DefaultValue,
        AutoApplyOnStart: true,
        PeriodicCheckMinutes: 10,
        StartWithWindows: false,
        CheckUpdatesOnStart: true,
        Tweaks: ExtraTweaks.None,
        ApplyTweaks: true);
}
