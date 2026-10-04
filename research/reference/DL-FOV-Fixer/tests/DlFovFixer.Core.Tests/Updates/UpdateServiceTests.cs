using System.Text;
using DlFovFixer.Core.Updates;

namespace DlFovFixer.Core.Tests.Updates;

public sealed class UpdateServiceTests
{
    private const string Hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    private readonly FakeChannel _channel = new();
    private readonly FakeInstaller _installer = new();
    private bool _signatureValid = true;

    [Fact]
    public async Task Check_NoManifestKey_IsNotConfiguredAndFetchesNothing()
    {
        var result = await Service(configured: false).CheckAsync(TestContext.Current.CancellationToken);

        Assert.IsType<UpdateCheckResult.NotConfigured>(result);
        Assert.Equal(0, _channel.Fetches);
    }

    [Fact]
    public async Task Check_DevBuild_NeverUpdatesItself()
    {
        var result = await Service(installed: "2.0.0-dev").CheckAsync(TestContext.Current.CancellationToken);

        Assert.IsType<UpdateCheckResult.DevelopmentBuild>(result);
        Assert.Equal(0, _channel.Fetches);
    }

    [Fact]
    public async Task Check_NewerRelease_IsAvailableWithItsInstaller()
    {
        _channel.Manifest = Manifest("2.0.1");

        var result = await Service().CheckAsync(TestContext.Current.CancellationToken);

        var available = Assert.IsType<UpdateCheckResult.Available>(result);
        Assert.Equal("2.0.1/DL-FOV-Fixer-2.0.1-setup.exe", available.Installer.Path);
    }

    [Fact]
    public async Task Check_SameRelease_IsUpToDate()
    {
        _channel.Manifest = Manifest("2.0.0");

        var result = await Service().CheckAsync(TestContext.Current.CancellationToken);

        Assert.Equal(new UpdateCheckResult.UpToDate("2.0.0"), result);
    }

    [Fact]
    public async Task Check_ReleaseWithoutAnInstaller_IsUpToDate()
    {
        _channel.Manifest = Manifest("2.0.1", kind: "portable");

        var result = await Service().CheckAsync(TestContext.Current.CancellationToken);

        Assert.IsType<UpdateCheckResult.UpToDate>(result);
    }

    [Fact]
    public async Task Check_SignatureDoesNotMatch_IsRefusedBeforeTheManifestIsRead()
    {
        _channel.Manifest = Manifest("2.0.1");
        _signatureValid = false;

        var result = await Service().CheckAsync(TestContext.Current.CancellationToken);

        Assert.IsType<UpdateCheckResult.BadSignature>(result);
    }

    [Fact]
    public async Task Check_SignatureIsCheckedOverTheExactManifestBytes()
    {
        _channel.Manifest = Manifest("2.0.1");
        byte[]? checkedBytes = null;
        string? checkedSignature = null;
        var service = new UpdateService("2.0.0", true, _channel, new FakeSignatures((bytes, signature) =>
        {
            checkedBytes = bytes;
            checkedSignature = signature;
            return true;
        }), _installer);

        await service.CheckAsync(TestContext.Current.CancellationToken);

        Assert.Equal(Encoding.UTF8.GetBytes(_channel.Manifest), checkedBytes);
        Assert.Equal("c2lnbmF0dXJl", checkedSignature);
    }

    [Fact]
    public async Task Check_BrokenManifest_SaysWhy()
    {
        _channel.Manifest = "{}";

        var result = await Service().CheckAsync(TestContext.Current.CancellationToken);

        Assert.Equal(new UpdateCheckResult.BadManifest("unsupported schema"), result);
    }

    [Fact]
    public async Task Check_NetworkFails_IsFailed()
    {
        _channel.Failure = new HttpRequestException("offline");

        var result = await Service().CheckAsync(TestContext.Current.CancellationToken);

        Assert.Equal(new UpdateCheckResult.Failed("offline"), result);
    }

    [Fact]
    public async Task Install_MatchingSignedDownload_StartsTheInstaller()
    {
        var update = await Available();
        _channel.Download = new DownloadedArtifact(@"C:\Temp\setup.exe", 1234, Hash.ToUpperInvariant());

        var result = await Service().InstallAsync(update, TestContext.Current.CancellationToken);

        Assert.IsType<InstallResult.InstallerStarted>(result);
        Assert.Equal([@"C:\Temp\setup.exe"], _installer.Launched);
    }

    [Theory]
    [InlineData(1233, Hash)]
    [InlineData(1234, "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")]
    public async Task Install_DownloadDiffersFromTheManifest_IsNeverStarted(long size, string sha256)
    {
        var update = await Available();
        _channel.Download = new DownloadedArtifact(@"C:\Temp\setup.exe", size, sha256);

        var result = await Service().InstallAsync(update, TestContext.Current.CancellationToken);

        Assert.IsType<InstallResult.DownloadCorrupted>(result);
        Assert.Empty(_installer.Launched);
    }

    private async Task<UpdateCheckResult.Available> Available()
    {
        _channel.Manifest = Manifest("2.0.1");
        return Assert.IsType<UpdateCheckResult.Available>(await Service().CheckAsync(TestContext.Current.CancellationToken));
    }

    private UpdateService Service(bool configured = true, string installed = "2.0.0") =>
        new(installed, configured, _channel, new FakeSignatures((_, _) => _signatureValid), _installer);

    private static string Manifest(string version, string kind = "installer") => $$"""
        {"schema":1,"version":"{{version}}","publishedAt":"2026-10-01T12:00:00Z",
         "artifacts":[{"kind":"{{kind}}","path":"{{version}}/DL-FOV-Fixer-{{version}}-setup.exe","size":1234,"sha256":"{{Hash}}"}]}
        """;

    private sealed class FakeChannel : IReleaseChannel
    {
        public string Manifest { get; set; } = "{}";

        public DownloadedArtifact? Download { get; set; }

        public Exception? Failure { get; set; }

        public int Fetches { get; private set; }

        public Task<ChannelSnapshot> FetchLatestAsync(CancellationToken cancellationToken)
        {
            Fetches++;
            return Failure is null
                ? Task.FromResult(new ChannelSnapshot(Encoding.UTF8.GetBytes(Manifest), "c2lnbmF0dXJl"))
                : Task.FromException<ChannelSnapshot>(Failure);
        }

        public Task<DownloadedArtifact> DownloadAsync(string path, CancellationToken cancellationToken) =>
            Task.FromResult(Download!);
    }

    private sealed class FakeSignatures(Func<byte[], string, bool> verify) : ISignatureVerifier
    {
        public bool Verify(ReadOnlySpan<byte> data, string signatureBase64) => verify(data.ToArray(), signatureBase64);
    }

    private sealed class FakeInstaller : IUpdateInstaller
    {
        public List<string> Launched { get; } = [];

        public void Launch(string localPath) => Launched.Add(localPath);
    }
}
