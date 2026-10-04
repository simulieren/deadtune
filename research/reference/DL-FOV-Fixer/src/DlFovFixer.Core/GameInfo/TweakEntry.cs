namespace DlFovFixer.Core.GameInfo;

/// <summary>One key and value to write into a block, in the order the user gave it.</summary>
public sealed record TweakEntry(string Key, string Value);
