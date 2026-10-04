namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// A pasted config sorted into its three destinations, in first-seen order. <see cref="Value"/> is
/// the pasted <c>r_aspectratio</c>, kept apart because the chosen value is the only source of truth.
/// </summary>
public sealed record ParsedTweaks(
    string? Value,
    IReadOnlyList<TweakEntry> ConVars,
    IReadOnlyList<TweakEntry> SceneSystem,
    IReadOnlyList<TweakEntry> Video);
