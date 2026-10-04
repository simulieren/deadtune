using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Tests.Vectors;

namespace DlFovFixer.Core.Tests.GameInfo;

public sealed class BlockMergeVectorTests
{
    private const string File = "gameinfo-merge.json";

    public static TheoryData<string, bool> Cases => VectorFile.CasesWithLineEndings(File);

    [Theory]
    [MemberData(nameof(Cases))]
    public void Merge_MatchesTheVector_AndASecondMergeChangesNothing(string name, bool crlf)
    {
        var vector = VectorFile.Case(File, name);
        var expected = vector.GetProperty("expected");
        var entries = VectorFile.Entries(vector.GetProperty("entries"));
        var block = vector.GetProperty("block").GetString()!;
        var quoted = vector.GetProperty("quoted").GetBoolean();
        var create = vector.GetProperty("create").GetBoolean();

        var outcome = BlockMerge.Merge(VectorFile.Text(vector.GetProperty("input"), crlf)!, block, entries, quoted, create);

        Assert.Equal(VectorFile.Text(expected.GetProperty("output"), crlf), outcome.Text);
        Assert.Equal(VectorFile.PairTexts(expected.GetProperty("results")), VectorFile.PairTexts(outcome.Keys));
        Assert.Equal(outcome.Text, BlockMerge.Merge(outcome.Text, block, entries, quoted, create).Text);
    }
}
