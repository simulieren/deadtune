using System.Text.Json;
using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Tests.Vectors;

namespace DlFovFixer.Core.Tests.GameInfo;

public sealed class AspectRatioVectorTests
{
    private static readonly JsonElement Vectors = VectorFile.Load("fov-value.json");

    public static TheoryData<string> NormalizeInputs => Inputs("normalize");

    public static TheoryData<string> DegreeInputs => Inputs("toDegrees");

    [Theory]
    [MemberData(nameof(NormalizeInputs))]
    public void Normalize_MatchesTheVector(string input)
    {
        var expected = VectorFile.String(Expected("normalize", input));

        Assert.Equal(expected, AspectRatio.Normalize(input));
    }

    [Theory]
    [MemberData(nameof(DegreeInputs))]
    public void ToFovDegrees_MatchesTheVector(string input)
    {
        var expected = Expected("toDegrees", input);

        Assert.Equal(expected.ValueKind == JsonValueKind.Null ? null : expected.GetInt32(), AspectRatio.ToFovDegrees(input));
    }

    [Fact]
    public void PresetsAndDefault_MatchTheVector()
    {
        var expected = Vectors.GetProperty("presets").EnumerateArray()
            .Select(p => new FovPreset(p.GetProperty("degrees").GetInt32(), p.GetProperty("value").GetString()!));

        // The vector is the expectation here, so the constant goes through a local on the actual side.
        var defaultValue = AspectRatio.DefaultValue;

        Assert.Equal(expected, FovPresets.All);
        Assert.Equal(Vectors.GetProperty("defaultValue").GetString(), defaultValue);
    }

    private static TheoryData<string> Inputs(string section) =>
        [.. Vectors.GetProperty(section).EnumerateArray().Select(item => item.GetProperty("input").GetString()!)];

    private static JsonElement Expected(string section, string input) =>
        Vectors.GetProperty(section).EnumerateArray()
            .Single(item => item.GetProperty("input").GetString() == input)
            .GetProperty("expected");
}
