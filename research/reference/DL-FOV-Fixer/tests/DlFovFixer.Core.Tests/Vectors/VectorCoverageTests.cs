namespace DlFovFixer.Core.Tests.Vectors;

/// <summary>A vector file nobody reads proves nothing, so a new file must get a runner here.</summary>
public sealed class VectorCoverageTests
{
    [Fact]
    public void EveryVectorFile_HasARunner()
    {
        string[] read =
        [
            "fov-value.json",
            "gameinfo-apply.json",
            "gameinfo-merge.json",
            "release-channel.json",
            "semantic-version.json",
            "tweak-parsing.json",
            "video-config.json",
        ];

        var present = Directory.GetFiles(VectorFile.Directory, "*.json").Select(Path.GetFileName).Order(StringComparer.Ordinal);

        Assert.Equal(read, present);
    }
}
