using System.Text;
using DlFovFixer.App.Tests.Support;
using DlFovFixer.App.ViewModels;
using DlFovFixer.Core.Updates;

namespace DlFovFixer.App.Tests.ViewModels;

public sealed class UpdatesViewModelTests
{
    private const string Hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    private const string ReleasesPage = "https://github.com/lukr-99/DL-FOV-Fixer/releases/latest";

    private readonly FakeShell _shell = new();
    private readonly FakeChannel _channel = new();
    private readonly List<string> _launched = [];
    private bool _configured = true;
    private string _installed = "2.0.0";

    [Fact]
    public async Task Check_OnStartWithANewerRelease_OnlyNotifies()
    {
        _channel.Manifest = Manifest("2.0.1");
        var model = Create();

        await model.CheckAsync(interactive: false, TestContext.Current.CancellationToken);

        Assert.Equal("Update available: DL-FOV-Fixer 2.0.1.", _shell.Notifications.Single());
        Assert.True(model.CanInstall);
        Assert.Equal("Install update 2.0.1", model.InstallLabel);
        Assert.Empty(_launched);
    }

    [Fact]
    public async Task Check_AskedAndAccepted_InstallsAndAsksTheAppToExit()
    {
        _channel.Manifest = Manifest("2.0.1");
        _shell.Confirms = true;
        var model = Create();
        var exits = 0;
        model.ExitRequested += (_, _) => exits++;

        await model.CheckAsync(interactive: true, TestContext.Current.CancellationToken);

        Assert.Equal([@"C:\Temp\setup.exe"], _launched);
        Assert.Equal(1, exits);
    }

    [Fact]
    public async Task Check_AskedAndDeclined_KeepsTheOfferInTheMenu()
    {
        _channel.Manifest = Manifest("2.0.1");
        var model = Create();

        await model.CheckAsync(interactive: true, TestContext.Current.CancellationToken);

        Assert.Empty(_launched);
        Assert.True(model.CanInstall);
    }

    [Fact]
    public async Task Check_OnStartUpToDate_StaysQuiet()
    {
        _channel.Manifest = Manifest("2.0.0");
        var model = Create();

        await model.CheckAsync(interactive: false, TestContext.Current.CancellationToken);

        Assert.Empty(_shell.Notifications);
        Assert.False(model.CanInstall);
    }

    [Fact]
    public async Task Check_AskedUpToDate_SaysSo()
    {
        _channel.Manifest = Manifest("2.0.0");

        await Create().CheckAsync(interactive: true, TestContext.Current.CancellationToken);

        Assert.Equal("No update found. You have the latest version, 2.0.0.", _shell.Notifications.Single());
    }

    [Theory]
    [InlineData(false, "2.0.0")]
    [InlineData(true, "2.0.0-dev")]
    public async Task Check_BuildThatCannotUpdate_OffersTheReleasesPage(bool configured, string installed)
    {
        _configured = configured;
        _installed = installed;
        _shell.Confirms = true;

        await Create().CheckAsync(interactive: true, TestContext.Current.CancellationToken);

        Assert.Equal([ReleasesPage], _shell.Opened);
        Assert.Equal(0, _channel.Fetches);
    }

    [Fact]
    public async Task Check_OnStartInABuildThatCannotUpdate_DoesNothing()
    {
        _configured = false;

        await Create().CheckAsync(interactive: false, TestContext.Current.CancellationToken);

        Assert.Empty(_shell.Notifications);
        Assert.Empty(_shell.Opened);
    }

    [Fact]
    public async Task Check_Offline_SaysSoOnlyWhenAsked()
    {
        _channel.Failure = new InvalidOperationException("offline");
        var model = Create();

        await model.CheckAsync(interactive: false, TestContext.Current.CancellationToken);
        Assert.Empty(_shell.Notifications);

        await model.CheckAsync(interactive: true, TestContext.Current.CancellationToken);
        Assert.Equal("Update check failed: offline", _shell.Notifications.Single());
        Assert.False(model.IsChecking);
    }

    [Fact]
    public async Task Install_DownloadDoesNotMatch_InstallsNothing()
    {
        _channel.Manifest = Manifest("2.0.1");
        var model = Create();
        await model.CheckAsync(interactive: false, TestContext.Current.CancellationToken);
        _channel.Download = new DownloadedArtifact(@"C:\Temp\setup.exe", 999, Hash);

        await model.InstallAsync(TestContext.Current.CancellationToken);

        Assert.Empty(_launched);
        Assert.Equal("The download does not match the release. Nothing was installed.", _shell.Notifications[^1]);
        Assert.True(model.CanInstall);
    }

    [Fact]
    public async Task Install_NothingAvailable_SaysWhatToDo()
    {
        await Create().InstallAsync(TestContext.Current.CancellationToken);

        Assert.Contains("Check for updates", _shell.Notifications.Single(), StringComparison.Ordinal);
    }

    private UpdatesViewModel Create()
    {
        var service = new UpdateService(_installed, _configured, _channel, new TrustAll(), new RecordingInstaller(_launched));
        return new UpdatesViewModel(service, _installed, ReleasesPage, _shell, _shell, _shell);
    }

    private static string Manifest(string version) => $$"""
        {"schema":1,"version":"{{version}}","publishedAt":"2026-10-01T12:00:00Z",
         "artifacts":[{"kind":"installer","path":"{{version}}/DL-FOV-Fixer-{{version}}-setup.exe","size":1234,"sha256":"{{Hash}}"}]}
        """;

    private sealed class FakeChannel : IReleaseChannel
    {
        public string Manifest { get; set; } = "{}";

        public DownloadedArtifact Download { get; set; } = new(@"C:\Temp\setup.exe", 1234, Hash);

        public Exception? Failure { get; set; }

        public int Fetches { get; private set; }

        public Task<ChannelSnapshot> FetchLatestAsync(CancellationToken cancellationToken)
        {
            Fetches++;
            return Failure is null
                ? Task.FromResult(new ChannelSnapshot(Encoding.UTF8.GetBytes(Manifest), "c2ln"))
                : Task.FromException<ChannelSnapshot>(Failure);
        }

        public Task<DownloadedArtifact> DownloadAsync(string path, CancellationToken cancellationToken) => Task.FromResult(Download);
    }

    private sealed class TrustAll : ISignatureVerifier
    {
        public bool Verify(ReadOnlySpan<byte> data, string signatureBase64) => true;
    }

    private sealed class RecordingInstaller(List<string> launched) : IUpdateInstaller
    {
        public void Launch(string localPath) => launched.Add(localPath);
    }
}
