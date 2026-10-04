using System.ComponentModel;
using System.Diagnostics;

namespace DlFovFixer.App.Shell;

/// <summary>
/// Opens a file with its associated program. A .gi file usually has none, so Notepad is the fallback.
/// </summary>
public sealed class ShellFileOpener : IFileOpener
{
    public void Open(string path)
    {
        try
        {
            Process.Start(new ProcessStartInfo(path) { UseShellExecute = true })?.Dispose();
        }
        catch (Win32Exception)
        {
            Process.Start(new ProcessStartInfo("notepad.exe") { ArgumentList = { path } })?.Dispose();
        }
    }
}
