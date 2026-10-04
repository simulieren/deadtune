namespace DlFovFixer.Core.Updates;

/// <summary>What happened when installing an available update.</summary>
public abstract record InstallResult
{
    private InstallResult()
    {
    }

    public sealed record InstallerStarted : InstallResult;

    /// <summary>The download's size or SHA-256 differs from the manifest.</summary>
    public sealed record DownloadCorrupted : InstallResult;

    public sealed record Failed(string Detail) : InstallResult;
}
