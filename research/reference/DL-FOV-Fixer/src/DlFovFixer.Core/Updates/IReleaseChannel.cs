namespace DlFovFixer.Core.Updates;

/// <summary>Reads the update channel, the public GitHub Releases. Throws on network or access errors.</summary>
public interface IReleaseChannel
{
    /// <summary>The latest release's manifest.json, as exact bytes, and its detached signature.</summary>
    Task<ChannelSnapshot> FetchLatestAsync(CancellationToken cancellationToken);

    /// <summary>Downloads the artifact at <paramref name="path"/> to a private file and returns it.</summary>
    Task<DownloadedArtifact> DownloadAsync(string path, CancellationToken cancellationToken);
}
