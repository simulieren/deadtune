using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Tests.Vectors;

namespace DlFovFixer.Core.Tests.GameInfo;

public sealed class TweakTextParserVectorTests
{
    private const string File = "tweak-parsing.json";

    public static TheoryData<string> ParseCases => VectorFile.CaseNames(File);

    public static TheoryData<string> MergeListCases => VectorFile.CaseNames(File, "mergeLists");

    [Theory]
    [MemberData(nameof(ParseCases))]
    public void Parse_MatchesTheVector(string name)
    {
        var vector = VectorFile.Case(File, name);
        var expected = vector.GetProperty("expected");

        var parsed = TweakTextParser.Parse(VectorFile.Text(vector.GetProperty("input"))!);

        Assert.Equal(VectorFile.String(expected.GetProperty("fov")), parsed.Value);
        Assert.Equal(VectorFile.PairTexts(expected.GetProperty("convars")), VectorFile.PairTexts(parsed.ConVars));
        Assert.Equal(VectorFile.PairTexts(expected.GetProperty("scenesystem")), VectorFile.PairTexts(parsed.SceneSystem));
        Assert.Equal(VectorFile.PairTexts(expected.GetProperty("video")), VectorFile.PairTexts(parsed.Video));
    }

    [Theory]
    [MemberData(nameof(MergeListCases))]
    public void MergeLists_MatchesTheVector(string name)
    {
        var vector = VectorFile.Load(File).GetProperty("mergeLists").EnumerateArray()
            .Single(item => item.GetProperty("name").GetString() == name);

        var merged = TweakTextParser.MergeLists(
            VectorFile.Entries(vector.GetProperty("existing")),
            VectorFile.Entries(vector.GetProperty("incoming")));

        Assert.Equal(VectorFile.PairTexts(vector.GetProperty("expected")), VectorFile.PairTexts(merged));
    }
}
