namespace DlFovFixer.Core.GameInfo;

/// <summary>The outcome for one key of a merge.</summary>
public sealed record KeyMerge(string Key, MergeAction Action);
