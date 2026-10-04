namespace DlFovFixer.Core.GameInfo;

/// <summary>
/// The result of applying the value and tweaks to gameinfo.gi. <see cref="Changed"/> is false when
/// the text is exactly what it was, and then nothing should be written.
/// </summary>
public sealed record GameInfoMergeOutcome(
    string Text,
    bool Changed,
    string? PreviousValue,
    IReadOnlyList<KeyMerge> ConVars,
    IReadOnlyList<KeyMerge> SceneSystem);
