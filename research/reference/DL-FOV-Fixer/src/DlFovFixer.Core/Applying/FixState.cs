namespace DlFovFixer.Core.Applying;

/// <summary>Where the fix stands. The tray shows three colors, and each state maps to one of them.</summary>
public enum FixState
{
    /// <summary>The target value is in the file.</summary>
    Ok,

    /// <summary>The file was found, but it has no <c>r_aspectratio</c>.</summary>
    NotApplied,

    /// <summary>The file was found, and its value differs from the target, usually after a game update.</summary>
    Drifted,

    /// <summary>The game has the file open. The next apply tries again.</summary>
    Waiting,

    /// <summary>There is no gameinfo.gi at the stored path.</summary>
    Missing,

    /// <summary>A file could not be read or written for any other reason.</summary>
    Failed,
}
