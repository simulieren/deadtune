namespace DlFovFixer.App.Shell;

/// <summary>Shows a short message next to the tray icon.</summary>
public interface INotifier
{
    void Notify(string message);
}
