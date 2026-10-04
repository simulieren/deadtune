using DlFovFixer.App.Shell;
using DlFovFixer.Core.Updates;

namespace DlFovFixer.App.ViewModels;

/// <summary>
/// "Check for updates" and "Install update" in the tray menu. A build that cannot update itself
/// offers the releases page instead. <see cref="ExitRequested"/> fires once an installer started,
/// so the app gets out of its way.
/// </summary>
public sealed class UpdatesViewModel(
    UpdateService service,
    string installedVersion,
    string releasesPage,
    IUserPrompts prompts,
    INotifier notifier,
    IFileOpener opener)
{
    public event EventHandler? Changed;

    public event EventHandler? ExitRequested;

    public bool IsChecking { get; private set; }

    public bool IsInstalling { get; private set; }

    public UpdateCheckResult.Available? Available { get; private set; }

    public string CheckLabel => IsChecking ? "Checking for updates…" : "Check for updates";

    public string InstallLabel => Available is { } update ? $"Install update {update.Manifest.Version}" : "Install update";

    public bool CanInstall => Available is not null && !IsInstalling;

    /// <summary>Checks the channel. A check the user asked for reports every outcome and offers to install.</summary>
    public async Task CheckAsync(bool interactive, CancellationToken cancellationToken = default)
    {
        if (IsChecking)
        {
            if (interactive)
            {
                notifier.Notify("An update check is already running.");
            }

            return;
        }

        SetChecking(true);
        UpdateCheckResult result;
        try
        {
            result = await service.CheckAsync(cancellationToken);
        }
        finally
        {
            SetChecking(false);
        }

        switch (result)
        {
            case UpdateCheckResult.Available available:
                Available = available;
                Changed?.Invoke(this, EventArgs.Empty);
                notifier.Notify($"Update available: DL-FOV-Fixer {available.Manifest.Version}.");
                if (interactive && prompts.Confirm(Offer(available)))
                {
                    await InstallAsync(cancellationToken);
                }

                break;
            case UpdateCheckResult.UpToDate when interactive:
                notifier.Notify($"No update found. You have the latest version, {installedVersion}.");
                break;
            case UpdateCheckResult.NotConfigured or UpdateCheckResult.DevelopmentBuild when interactive:
                if (prompts.Confirm($"This build ({installedVersion}) does not update itself.\n\nOpen the releases page to download the latest version?"))
                {
                    opener.Open(releasesPage);
                }

                break;
            case UpdateCheckResult.BadSignature when interactive:
                notifier.Notify("The latest release is not signed with the app's key. Nothing was downloaded.");
                break;
            case UpdateCheckResult.BadManifest bad when interactive:
                notifier.Notify($"The latest release did not pass the app's checks ({bad.Reason}). Nothing was downloaded.");
                break;
            case UpdateCheckResult.Failed failed when interactive:
                notifier.Notify($"Update check failed: {failed.Detail}");
                break;
        }
    }

    public async Task InstallAsync(CancellationToken cancellationToken = default)
    {
        if (Available is not { } update)
        {
            notifier.Notify("No update is available yet. Use 'Check for updates' first.");
            return;
        }

        if (IsInstalling)
        {
            return;
        }

        IsInstalling = true;
        Changed?.Invoke(this, EventArgs.Empty);
        notifier.Notify($"Downloading DL-FOV-Fixer {update.Manifest.Version}…");
        InstallResult result;
        try
        {
            result = await service.InstallAsync(update, cancellationToken);
        }
        finally
        {
            IsInstalling = false;
            Changed?.Invoke(this, EventArgs.Empty);
        }

        switch (result)
        {
            case InstallResult.InstallerStarted:
                ExitRequested?.Invoke(this, EventArgs.Empty);
                break;
            case InstallResult.DownloadCorrupted:
                notifier.Notify("The download does not match the release. Nothing was installed.");
                break;
            case InstallResult.Failed failed:
                notifier.Notify($"Update install failed: {failed.Detail}");
                break;
        }
    }

    private string Offer(UpdateCheckResult.Available available)
    {
        var size = available.Installer.Size / 1024.0 / 1024.0;
        return $"DL-FOV-Fixer {available.Manifest.Version} is available.\n\n" +
            $"Current version: {installedVersion}\n" +
            $"Download: {size:0.0} MB\n\n" +
            "Download and install it now? The app closes while the installer runs and starts again afterwards.";
    }

    private void SetChecking(bool checking)
    {
        IsChecking = checking;
        Changed?.Invoke(this, EventArgs.Empty);
    }
}
