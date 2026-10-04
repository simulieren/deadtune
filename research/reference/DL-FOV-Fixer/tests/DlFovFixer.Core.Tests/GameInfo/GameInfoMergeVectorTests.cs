using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Tests.Vectors;

namespace DlFovFixer.Core.Tests.GameInfo;

public sealed class GameInfoMergeVectorTests
{
    private const string File = "gameinfo-apply.json";

    public static TheoryData<string, bool> Cases => VectorFile.CasesWithLineEndings(File);

    [Theory]
    [MemberData(nameof(Cases))]
    public void Apply_MatchesTheVector_AndASecondApplyChangesNothing(string name, bool crlf)
    {
        var vector = VectorFile.Case(File, name);
        var expected = vector.GetProperty("expected");
        var value = vector.GetProperty("fov").GetString();
        var conVars = VectorFile.Entries(vector.GetProperty("convars"));
        var sceneSystem = VectorFile.Entries(vector.GetProperty("scenesystem"));
        var applyTweaks = vector.GetProperty("applyTweaks").GetBoolean();

        var outcome = GameInfoMerge.Apply(VectorFile.Text(vector.GetProperty("input"), crlf)!, value, conVars, sceneSystem, applyTweaks);

        Assert.Equal(VectorFile.Text(expected.GetProperty("output"), crlf), outcome.Text);
        Assert.Equal(expected.GetProperty("changed").GetBoolean(), outcome.Changed);
        Assert.Equal(VectorFile.String(expected.GetProperty("previousFov")), outcome.PreviousValue);
        Assert.Equal(VectorFile.PairTexts(expected.GetProperty("convars")), VectorFile.PairTexts(outcome.ConVars));
        Assert.Equal(VectorFile.PairTexts(expected.GetProperty("scenesystem")), VectorFile.PairTexts(outcome.SceneSystem));

        var again = GameInfoMerge.Apply(outcome.Text, value, conVars, sceneSystem, applyTweaks);
        Assert.False(again.Changed);
        Assert.Equal(outcome.Text, again.Text);
    }
}
