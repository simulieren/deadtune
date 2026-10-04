using System.Net;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using DlFovFixer.Core.Updates;
using DlFovFixer.Infrastructure.Tests.Support;
using DlFovFixer.Infrastructure.Updates;

namespace DlFovFixer.Infrastructure.Tests.Updates;

public sealed class GitHubReleaseChannelTests : IDisposable
{
    private const string Repository = "https://github.com/lukr-99/DL-FOV-Fixer";

    private readonly TempFolder _folder = new();
    private readonly FakeHandler _handler = new();

    public void Dispose() => _folder.Dispose();

    [Fact]
    public async Task FetchLatest_ReadsTheManifestAndItsSignature()
    {
        _handler.Responses[$"{Repository}/releases/latest/download/manifest.json"] = Encoding.UTF8.GetBytes("{\"schema\":1}\n");
        _handler.Responses[$"{Repository}/releases/latest/download/manifest.sig"] = Encoding.ASCII.GetBytes("c2lnbmF0dXJl\r\n");

        var snapshot = await Channel().FetchLatestAsync(TestContext.Current.CancellationToken);

        Assert.Equal("{\"schema\":1}\n", Encoding.UTF8.GetString(snapshot.ManifestBytes));
        Assert.Equal("c2lnbmF0dXJl", snapshot.SignatureBase64);
    }

    [Fact]
    public async Task FetchLatest_NoSignature_Throws()
    {
        _handler.Responses[$"{Repository}/releases/latest/download/manifest.json"] = Encoding.UTF8.GetBytes("{}");

        await Assert.ThrowsAsync<HttpRequestException>(() => Channel().FetchLatestAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task FetchLatest_FarTooLarge_IsRefused()
    {
        _handler.Responses[$"{Repository}/releases/latest/download/manifest.json"] = new byte[GitHubReleaseChannel.MaxManifestSize + 1];
        _handler.Responses[$"{Repository}/releases/latest/download/manifest.sig"] = [1];

        await Assert.ThrowsAsync<InvalidDataException>(() => Channel().FetchLatestAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task FetchLatest_NotFound_Throws()
    {
        await Assert.ThrowsAsync<HttpRequestException>(() => Channel().FetchLatestAsync(TestContext.Current.CancellationToken));
    }

    [Fact]
    public async Task Download_WritesTheFileAndMeasuresIt()
    {
        var content = Encoding.UTF8.GetBytes("installer bytes");
        _handler.Responses[$"{Repository}/releases/download/v2.0.1/setup.exe"] = content;

        var downloaded = await Channel().DownloadAsync("2.0.1/setup.exe", TestContext.Current.CancellationToken);

        Assert.Equal(_folder.File("updates", "setup.exe"), downloaded.LocalPath);
        Assert.Equal(content, File.ReadAllBytes(downloaded.LocalPath));
        Assert.Equal(content.Length, downloaded.Size);
        Assert.Equal(Convert.ToHexStringLower(SHA256.HashData(content)), downloaded.Sha256);
    }

    [Fact]
    public async Task Download_RemovesAnEarlierDownload()
    {
        var old = _folder.WriteBytes(Path.Combine("updates", "old-setup.exe"), [1, 2, 3]);
        _handler.Responses[$"{Repository}/releases/download/v2.0.1/setup.exe"] = [4];

        await Channel().DownloadAsync("2.0.1/setup.exe", TestContext.Current.CancellationToken);

        Assert.False(File.Exists(old));
    }

    [Theory]
    [InlineData("2.0.1/../../evil.exe")]
    [InlineData("https://example.com/setup.exe")]
    public async Task Download_PathOutsideTheReleases_IsNeverRequested(string path)
    {
        await Assert.ThrowsAsync<InvalidOperationException>(() => Channel().DownloadAsync(path, TestContext.Current.CancellationToken));

        Assert.Empty(_handler.Requested);
    }

    private GitHubReleaseChannel Channel() =>
        new(new HttpClient(_handler), new ReleaseChannelAddress(Repository), _folder.File("updates"));

    private sealed class FakeHandler : HttpMessageHandler
    {
        public Dictionary<string, byte[]> Responses { get; } = [];

        public List<string> Requested { get; } = [];

        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            var url = request.RequestUri!.ToString();
            Requested.Add(url);
            return Task.FromResult(Responses.TryGetValue(url, out var body)
                ? new HttpResponseMessage(HttpStatusCode.OK) { Content = new ByteArrayContent(body) }
                : new HttpResponseMessage(HttpStatusCode.NotFound));
        }
    }
}
