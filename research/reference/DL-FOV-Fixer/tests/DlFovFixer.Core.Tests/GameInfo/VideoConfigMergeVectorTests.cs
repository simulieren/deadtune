using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Tests.Vectors;

namespace DlFovFixer.Core.Tests.GameInfo;

public sealed class VideoConfigMergeVectorTests
{
    private const string File = "video-config.json";

    public static TheoryData<string, bool> Cases => VectorFile.CasesWithLineEndings(File);

    [Theory]
    [MemberData(nameof(Cases))]
    public void Merge_MatchesTheVector_AndASecondMergeChangesNothing(string name, bool crlf)
    {
        var vector = VectorFile.Case(File, name);
        var expected = vector.GetProperty("expected");
        var entries = VectorFile.Entries(vector.GetProperty("entries"));

        var outcome = VideoConfigMerge.Merge(VectorFile.Text(vector.GetProperty("input"), crlf), entries);

        Assert.Equal(VectorFile.Text(expected.GetProperty("output"), crlf), outcome.Text);
        Assert.Equal(expected.GetProperty("changed").GetBoolean(), outcome.Changed);
        Assert.Equal(expected.GetProperty("created").GetBoolean(), outcome.Created);
        Assert.Equal(VectorFile.PairTexts(expected.GetProperty("results")), VectorFile.PairTexts(outcome.Keys));

        var again = VideoConfigMerge.Merge(outcome.Text, entries);
        Assert.False(again.Changed);
        Assert.Equal(outcome.Text, again.Text);
    }
}
