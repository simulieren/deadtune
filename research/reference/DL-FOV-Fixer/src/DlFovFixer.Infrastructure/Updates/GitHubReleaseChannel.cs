using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using DlFovFixer.Core.Updates;

namespace DlFovFixer.Infrastructure.Updates;

/// <summary>
/// Reads the update channel from the public GitHub Releases, with no sign-in. This class only
/// fetches bytes: the update service checks the signature and decides whether to trust them. GitHub answers a release
/// download with a redirect to its file host, which the default handler follows.
/// </summary>
public sealed class GitHubReleaseChannel(HttpClient http, ReleaseChannelAddress address, string downloadFolder) : IReleaseChannel
{
    /// <summary>A manifest is a few hundred bytes. Anything far larger is not one.</summary>
    public const int MaxManifestSize = 65_536;

    /// <summary>A base64 DER ECDSA P-256 signature is under 100 bytes.</summary>
    public const int MaxSignatureSize = 1_024;

    private const int BufferSize = 81_920;

    public async Task<ChannelSnapshot> FetchLatestAsync(CancellationToken cancellationToken)
    {
        var manifest = await FetchSmallAsync(address.Manifest, MaxManifestSize, cancellationToken).ConfigureAwait(false);
        var signature = await FetchSmallAsync(address.Signature, MaxSignatureSize, cancellationToken).ConfigureAwait(false);
        return new ChannelSnapshot(manifest, Encoding.ASCII.GetString(signature).Trim());
    }

    private async Task<byte[]> FetchSmallAsync(string url, int limit, CancellationToken cancellationToken)
    {
        using var response = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var source = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        await using (source.ConfigureAwait(false))
        {
            using var copy = new MemoryStream();
            await CopyAsync(source, copy, limit, hash: null, cancellationToken).ConfigureAwait(false);
            return copy.ToArray();
        }
    }

    public async Task<DownloadedArtifact> DownloadAsync(string path, CancellationToken cancellationToken)
    {
        var url = address.Artifact(path)
            ?? throw new InvalidOperationException($"The release file '{path}' is not where a release keeps its files.");

        // Only the newest download is kept, so an old installer never piles up.
        if (Directory.Exists(downloadFolder))
        {
            Directory.Delete(downloadFolder, recursive: true);
        }

        Directory.CreateDirectory(downloadFolder);
        var target = Path.Combine(downloadFolder, Path.GetFileName(path));

        using var response = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, cancellationToken).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        long total;
        var source = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        await using (source.ConfigureAwait(false))
        {
            var file = new FileStream(target, FileMode.CreateNew, FileAccess.Write, FileShare.None, BufferSize, useAsync: true);
            await using (file.ConfigureAwait(false))
            {
                total = await CopyAsync(source, file, ReleaseManifestParser.MaxArtifactSize, hash, cancellationToken).ConfigureAwait(false);
            }
        }

        return new DownloadedArtifact(target, total, Convert.ToHexStringLower(hash.GetHashAndReset()));
    }

    private static async Task<long> CopyAsync(Stream source, Stream target, long limit, IncrementalHash? hash, CancellationToken cancellationToken)
    {
        var buffer = new byte[BufferSize];
        long total = 0;
        int read;
        while ((read = await source.ReadAsync(buffer, cancellationToken).ConfigureAwait(false)) > 0)
        {
            total += read;
            if (total > limit)
            {
                throw new InvalidDataException($"The download is larger than {limit} bytes.");
            }

            hash?.AppendData(buffer, 0, read);
            await target.WriteAsync(buffer.AsMemory(0, read), cancellationToken).ConfigureAwait(false);
        }

        return total;
    }
}
