namespace DlFovFixer.Core.GameInfo;

/// <summary>The merged text and what happened to each key, in the order the keys were given.</summary>
public sealed record BlockMergeOutcome(string Text, IReadOnlyList<KeyMerge> Keys);
