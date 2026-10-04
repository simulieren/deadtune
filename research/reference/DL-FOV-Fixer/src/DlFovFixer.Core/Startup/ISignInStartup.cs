namespace DlFovFixer.Core.Startup;

/// <summary>Whether the app starts when the user signs in to Windows.</summary>
public interface ISignInStartup
{
    bool IsEnabled { get; }

    void SetEnabled(bool enabled);

    /// <summary>
    /// Points an existing startup entry at this copy of the app, so an entry 1.0 left behind starts
    /// the installed 2.0 rather than the old portable file. Does nothing when startup is off.
    /// </summary>
    void Repair();
}
