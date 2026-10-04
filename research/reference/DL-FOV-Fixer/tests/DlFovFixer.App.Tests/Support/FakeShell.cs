using DlFovFixer.App.Shell;
using DlFovFixer.Core.Startup;

namespace DlFovFixer.App.Tests.Support;

/// <summary>
/// The dialogs, the balloons, the file opener and sign-in startup, recorded. Each dialog answers
/// with the value set for it, which stands for what the user typed or picked.
/// </summary>
internal sealed class FakeShell : IUserPrompts, INotifier, IFileOpener, ISignInStartup
{
    public string? Value { get; set; }

    public string? PickedPath { get; set; }

    public string? PastedConfig { get; set; }

    public bool Confirms { get; set; }

    public List<string> Notifications { get; } = [];

    public List<string> Messages { get; } = [];

    public List<string> Opened { get; } = [];

    public int PathRequests { get; private set; }

    public (string Title, string Text)? Shown { get; private set; }

    public bool IsEnabled { get; private set; }

    public string? AskValue(string current) => Value;

    public string? AskGameInfoPath()
    {
        PathRequests++;
        return PickedPath;
    }

    public string? AskPastedConfig() => PastedConfig;

    public bool Confirm(string message) => Confirms;

    public void Inform(string message) => Messages.Add(message);

    public void ShowText(string title, string text) => Shown = (title, text);

    public void Notify(string message) => Notifications.Add(message);

    public void Open(string path) => Opened.Add(path);

    public void SetEnabled(bool enabled) => IsEnabled = enabled;

    public void Repair()
    {
    }
}
