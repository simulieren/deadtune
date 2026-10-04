using System.Diagnostics;
using DlFovFixer.Core.Updates;

namespace DlFovFixer.Infrastructure.Updates;

/// <summary>
/// Starts the checked Inno Setup installer as a quiet in-place update. The installer closes the app
/// through the Restart Manager if it still runs, installs over it and, because of /UPDATE, starts
/// it again. The app quits right after this returns.
/// </summary>
public sealed class InstallerLauncher : IUpdateInstaller
{
    public const string Arguments = "/SILENT /SUPPRESSMSGBOXES /NORESTART /UPDATE";

    public void Launch(string localPath)
    {
        using var process = Process.Start(new ProcessStartInfo(localPath)
        {
            Arguments = Arguments,
            UseShellExecute = true,
        });
    }
}
