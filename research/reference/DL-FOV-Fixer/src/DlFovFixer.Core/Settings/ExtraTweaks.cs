using DlFovFixer.Core.GameInfo;

namespace DlFovFixer.Core.Settings;

/// <summary>The extra tweaks the user pasted, stored per destination in first-seen order.</summary>
public sealed record ExtraTweaks(
    IReadOnlyList<TweakEntry> ConVars,
    IReadOnlyList<TweakEntry> SceneSystem,
    IReadOnlyList<TweakEntry> Video)
{
    public static ExtraTweaks None { get; } = new([], [], []);
}
