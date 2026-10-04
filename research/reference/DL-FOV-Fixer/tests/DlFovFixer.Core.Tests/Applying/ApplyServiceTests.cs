using DlFovFixer.Core.Applying;
using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Settings;

namespace DlFovFixer.Core.Tests.Applying;

public sealed class ApplyServiceTests
{
    private const string GameInfoPath = @"D:\Steam\steamapps\common\Deadlock\game\citadel\gameinfo.gi";
    private const string VideoPath = @"D:\Steam\steamapps\common\Deadlock\game\citadel\cfg\video.txt";
    private const string GameInfo = "\"GameInfo\"\r\n{\r\n\tConVars\r\n\t{\r\n\t\t\"r_aspectratio\" \"2.3\"\r\n\t}\r\n}\r\n";

    private readonly FakeGameFiles _files = new();

    [Fact]
    public void Apply_ValueDiffers_WritesGameInfoOnce()
    {
        _files.Texts[GameInfoPath] = GameInfo;

        var outcome = new ApplyService(_files).Apply(Settings());

        Assert.Equal(ApplyResult.Applied, outcome.Result);
        Assert.Equal([GameInfoPath], _files.Writes);
        Assert.Contains("\"r_aspectratio\" \"2.49\"", _files.Texts[GameInfoPath], StringComparison.Ordinal);
        Assert.Equal("2.3", outcome.GameInfo!.PreviousValue);
    }

    [Fact]
    public void Apply_Twice_WritesNothingTheSecondTime()
    {
        _files.Texts[GameInfoPath] = GameInfo;
        var service = new ApplyService(_files);
        var settings = Settings(video: [new("setting.fps_max", "240")]);
        service.Apply(settings);
        _files.Writes.Clear();

        var outcome = service.Apply(settings);

        Assert.Equal(ApplyResult.UpToDate, outcome.Result);
        Assert.Empty(_files.Writes);
    }

    [Fact]
    public void Apply_VideoTweaks_CreatesVideoTxtBesideGameInfo()
    {
        _files.Texts[GameInfoPath] = GameInfo;

        var outcome = new ApplyService(_files).Apply(Settings(video: [new("setting.fps_max", "240")]));

        Assert.Equal([GameInfoPath, VideoPath], _files.Writes);
        Assert.True(outcome.Video!.Created);
    }

    [Fact]
    public void Apply_TweaksOff_LeavesVideoTxtAlone()
    {
        _files.Texts[GameInfoPath] = GameInfo;

        new ApplyService(_files).Apply(Settings(video: [new("setting.fps_max", "240")]) with { ApplyTweaks = false });

        Assert.Equal([GameInfoPath], _files.Writes);
    }

    [Theory]
    [InlineData("")]
    [InlineData(@"D:\nowhere\gameinfo.gi")]
    public void Apply_NoGameInfo_IsMissing(string path)
    {
        var outcome = new ApplyService(_files).Apply(Settings() with { GameInfoPath = path });

        Assert.Equal(ApplyResult.Missing, outcome.Result);
        Assert.Empty(_files.Writes);
    }

    [Fact]
    public void Apply_GameHasTheFileOpen_IsWaiting()
    {
        _files.Texts[GameInfoPath] = GameInfo;
        _files.Failure = new GameFileException("in use", isLocked: true);

        var outcome = new ApplyService(_files).Apply(Settings());

        Assert.Equal(ApplyResult.Waiting, outcome.Result);
        Assert.Equal("in use", outcome.Error);
    }

    [Fact]
    public void Apply_WriteFailsForAnotherReason_IsFailed()
    {
        _files.Texts[GameInfoPath] = GameInfo;
        _files.Failure = new GameFileException("disk full", isLocked: false);

        var outcome = new ApplyService(_files).Apply(Settings());

        Assert.Equal(ApplyResult.Failed, outcome.Result);
    }

    private static AppSettings Settings(IReadOnlyList<TweakEntry>? video = null) =>
        AppSettings.Defaults with
        {
            GameInfoPath = GameInfoPath,
            FovValue = "2.49",
            Tweaks = ExtraTweaks.None with { Video = video ?? [] },
        };
}
