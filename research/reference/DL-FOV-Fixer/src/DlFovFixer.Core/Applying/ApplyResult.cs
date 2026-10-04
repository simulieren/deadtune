namespace DlFovFixer.Core.Applying;

/// <summary>How an apply ended.</summary>
public enum ApplyResult
{
    /// <summary>At least one file was written.</summary>
    Applied,

    /// <summary>Every file already matched, so nothing was written.</summary>
    UpToDate,

    /// <summary>There is no gameinfo.gi at the stored path.</summary>
    Missing,

    /// <summary>The game has a file open. The next apply tries again.</summary>
    Waiting,

    /// <summary>A file could not be read or written for any other reason.</summary>
    Failed,
}
