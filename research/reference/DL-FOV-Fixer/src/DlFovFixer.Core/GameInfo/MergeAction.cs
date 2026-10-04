namespace DlFovFixer.Core.GameInfo;

/// <summary>What a merge did with one key.</summary>
public enum MergeAction
{
    /// <summary>The key existed as a simple value and was set in place.</summary>
    Updated,

    /// <summary>The key was missing and was inserted at the top of the block.</summary>
    Added,

    /// <summary>The key exists as a nested sub-block, so it was left alone.</summary>
    SkippedBlock,

    /// <summary>The block does not exist and creating it was not allowed.</summary>
    NoBlock,

    /// <summary>The text has no root block to create the block in.</summary>
    NoRoot,
}
