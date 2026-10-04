namespace DlFovFixer.App.Shell;

/// <summary>The three colors of the tray icon, as 1.0 had them.</summary>
public enum StatusColor
{
    /// <summary>The fix is in place.</summary>
    Green,

    /// <summary>Not applied yet, changed by an update, or waiting for the game to close.</summary>
    Amber,

    /// <summary>The file is missing or could not be read or written.</summary>
    Red,
}
