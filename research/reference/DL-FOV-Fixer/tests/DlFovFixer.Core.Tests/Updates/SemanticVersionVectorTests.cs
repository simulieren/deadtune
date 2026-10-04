using System.Text.Json;
using DlFovFixer.Core.Tests.Vectors;
using DlFovFixer.Core.Updates;

namespace DlFovFixer.Core.Tests.Updates;

/// <summary>Runs contracts/vectors/semantic-version.json.</summary>
public sealed class SemanticVersionVectorTests
{
    private const string File = "semantic-version.json";

    public static TheoryData<string> ParseInputs() => Inputs("parse", "input");

    public static TheoryData<string> CompareCases() => Inputs("compare", "left", "right");

    public static TheoryData<string> OfferCases() => VectorFile.CaseNames(File, "offer");

    [Theory]
    [MemberData(nameof(ParseInputs))]
    public void Parse(string input)
    {
        var expected = Entry("parse", item => item.GetProperty("input").GetString() == input).GetProperty("expected");

        var version = SemanticVersion.Parse(input);

        if (expected.ValueKind == JsonValueKind.Null)
        {
            Assert.Null(version);
            return;
        }

        Assert.Equal(
            new SemanticVersion(
                expected.GetProperty("major").GetInt32(),
                expected.GetProperty("minor").GetInt32(),
                expected.GetProperty("patch").GetInt32(),
                VectorFile.String(expected.GetProperty("prerelease"))),
            version);
        Assert.Equal(input, version!.ToString());
    }

    [Theory]
    [MemberData(nameof(CompareCases))]
    public void Compare(string pair)
    {
        var item = Entry("compare", entry => Key(entry, "left", "right") == pair);
        var left = SemanticVersion.Parse(item.GetProperty("left").GetString())!;
        var right = SemanticVersion.Parse(item.GetProperty("right").GetString())!;

        Assert.Equal(item.GetProperty("expected").GetInt32(), Math.Sign(left.CompareTo(right)));
        Assert.Equal(-item.GetProperty("expected").GetInt32(), Math.Sign(right.CompareTo(left)));
    }

    [Theory]
    [MemberData(nameof(OfferCases))]
    public void Offer(string name)
    {
        var item = VectorFile.Case(File, name, "offer");

        var offered = UpdatePolicy.ShouldOffer(item.GetProperty("installed").GetString()!, item.GetProperty("available").GetString()!);

        Assert.Equal(item.GetProperty("expected").GetBoolean(), offered);
    }

    private static JsonElement Entry(string section, Func<JsonElement, bool> match) =>
        VectorFile.Load(File).GetProperty(section).EnumerateArray().Single(match);

    private static string Key(JsonElement item, params string[] names) =>
        string.Join(" vs ", names.Select(name => item.GetProperty(name).GetString()));

    private static TheoryData<string> Inputs(string section, params string[] names)
    {
        var data = new TheoryData<string>();
        foreach (var item in VectorFile.Load(File).GetProperty(section).EnumerateArray())
        {
            data.Add(Key(item, names));
        }

        return data;
    }
}
