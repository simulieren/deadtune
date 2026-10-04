namespace DlFovFixer.App.Shell;

/// <summary>The dialogs the tray menu opens. Each returns null or false when the user cancels.</summary>
public interface IUserPrompts
{
    /// <summary>Asks for an <c>r_aspectratio</c> value, starting from <paramref name="current"/>.</summary>
    string? AskValue(string current);

    /// <summary>Asks the user to pick gameinfo.gi.</summary>
    string? AskGameInfoPath();

    /// <summary>Asks for a config to paste.</summary>
    string? AskPastedConfig();

    bool Confirm(string message);

    void Inform(string message);

    /// <summary>Shows text the user can read and copy, but not change.</summary>
    void ShowText(string title, string text);
}
