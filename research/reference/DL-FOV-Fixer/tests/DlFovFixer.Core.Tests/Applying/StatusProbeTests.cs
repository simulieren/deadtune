using DlFovFixer.Core.Applying;
using DlFovFixer.Core.Settings;

namespace DlFovFixer.Core.Tests.Applying;

public sealed class StatusProbeTests
{
    private const string GameInfoPath = @"D:\Steam\steamapps\common\Deadlock\game\citadel\gameinfo.gi";

    private readonly FakeGameFiles _files = new();

    [Theory]
    [InlineData("\"GameInfo\"\n{\n\tConVars\n\t{\n\t\t\"r_aspectratio\" \"2.49\"\n\t}\n}\n", FixState.Ok, "2.49")]
    [InlineData("\"GameInfo\"\n{\n\tConVars\n\t{\n\t\t\"r_aspectratio\" \"2.3\"\n\t}\n}\n", FixState.Drifted, "2.3")]
    [InlineData("\"GameInfo\"\n{\n\tConVars\n\t{\n\t}\n}\n", FixState.NotApplied, null)]
    public void Probe_ReadsTheValueInTheFile(string text, FixState expected, string? current)
    {
        _files.Texts[GameInfoPath] = text;

        var status = new StatusProbe(_files).Probe(Settings());

        Assert.Equal(new FixStatus(expected, current), status);
    }

    [Theory]
    [InlineData("")]
    [InlineData(@"D:\nowhere\gameinfo.gi")]
    public void Probe_NoFile_IsMissing(string path)
    {
        var status = new StatusProbe(_files).Probe(Settings() with { GameInfoPath = path });

        Assert.Equal(FixState.Missing, status.State);
    }

    [Theory]
    [InlineData(true, FixState.Waiting)]
    [InlineData(false, FixState.Failed)]
    public void Probe_ReadFails_IsWaitingOnlyWhenLocked(bool isLocked, FixState expected)
    {
        _files.Texts[GameInfoPath] = string.Empty;
        _files.ReadFailure = new GameFileException("nope", isLocked);

        var status = new StatusProbe(_files).Probe(Settings());

        Assert.Equal(new FixStatus(expected, Error: "nope"), status);
    }

    [Theory]
    [InlineData(ApplyResult.Waiting, FixState.Waiting)]
    [InlineData(ApplyResult.Failed, FixState.Failed)]
    [InlineData(ApplyResult.Missing, FixState.Missing)]
    public void After_ApplyThatDidNotFinish_KeepsWhatItRanInto(ApplyResult result, FixState expected)
    {
        _files.Texts[GameInfoPath] = "\"GameInfo\"\n{\n\tConVars\n\t{\n\t\t\"r_aspectratio\" \"2.49\"\n\t}\n}\n";

        var status = new StatusProbe(_files).After(new ApplyOutcome(result, Error: "why"), Settings());

        Assert.Equal(expected, status.State);
    }

    [Fact]
    public void After_ApplyThatWrote_ReadsTheFileAgain()
    {
        _files.Texts[GameInfoPath] = "\"GameInfo\"\n{\n}\n";
        var service = new ApplyService(_files);
        var settings = Settings();

        var status = new StatusProbe(_files).After(service.Apply(settings), settings);

        Assert.Equal(new FixStatus(FixState.Ok, "2.49"), status);
    }

    private static AppSettings Settings() =>
        AppSettings.Defaults with { GameInfoPath = GameInfoPath, FovValue = "2.49" };
}
