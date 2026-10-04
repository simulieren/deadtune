namespace DlFovFixer.Core.GameInfo;

/// <summary>The indexes of a block's opening brace and its matching closing brace.</summary>
public readonly record struct BlockSpan(int Open, int Close);
