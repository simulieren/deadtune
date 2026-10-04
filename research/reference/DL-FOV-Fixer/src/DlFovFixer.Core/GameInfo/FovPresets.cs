namespace DlFovFixer.Core.GameInfo;

/// <summary>The presets shown in the tray menu, narrowest first.</summary>
public static class FovPresets
{
    public static IReadOnlyList<FovPreset> All { get; } =
    [
        new(80, "1.75"),
        new(90, "2.15"),
        new(100, "2.49"),
        new(105, "2.66"),
        new(110, "2.83"),
        new(115, "3.00"),
    ];
}
