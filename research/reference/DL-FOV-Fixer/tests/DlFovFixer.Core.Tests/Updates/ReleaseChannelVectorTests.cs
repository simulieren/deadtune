using System.Text;
using System.Text.Json;
using DlFovFixer.Core.Tests.Vectors;
using DlFovFixer.Core.Updates;

namespace DlFovFixer.Core.Tests.Updates;

/// <summary>Runs contracts/vectors/release-channel.json.</summary>
public sealed class ReleaseChannelVectorTests
{
    private const string File = "release-channel.json";

    private static readonly JsonElement Vectors = VectorFile.Load(File);

    private static readonly ReleaseChannelAddress Address = new(Vectors.GetProperty("repository").GetString()!);

    public static TheoryData<string> ArtifactPaths()
    {
        var data = new TheoryData<string>();
        foreach (var item in Vectors.GetProperty("artifacts").EnumerateArray())
        {
            data.Add(item.GetProperty("path").GetString()!);
        }

        return data;
    }

    public static TheoryData<string> ManifestCases() => VectorFile.CaseNames(File, "manifests");

    [Fact]
    public void Addresses_OfTheManifestItsSignatureAndTheReleasesPage()
    {
        Assert.Equal(Vectors.GetProperty("manifest").GetString(), Address.Manifest);
        Assert.Equal(Vectors.GetProperty("signature").GetString(), Address.Signature);
        Assert.Equal(Vectors.GetProperty("releasesPage").GetString(), Address.ReleasesPage);
    }

    [Theory]
    [MemberData(nameof(ArtifactPaths))]
    public void Artifact(string path)
    {
        var item = Vectors.GetProperty("artifacts").EnumerateArray().Single(entry => entry.GetProperty("path").GetString() == path);

        Assert.Equal(VectorFile.String(item.GetProperty("expected")), Address.Artifact(path));
    }

    [Theory]
    [MemberData(nameof(ManifestCases))]
    public void Manifest(string name)
    {
        var item = VectorFile.Case(File, name, "manifests");
        var manifest = item.GetProperty("manifest");
        var text = manifest.ValueKind == JsonValueKind.String ? manifest.GetString()! : manifest.GetRawText();

        var check = ReleaseManifestParser.Parse(Encoding.UTF8.GetBytes(text));

        Assert.Equal(item.GetProperty("valid").GetBoolean(), check is ManifestCheck.Valid);
        if (check is ManifestCheck.Valid { Manifest: var parsed })
        {
            // Every accepted artifact is one the channel agrees to download.
            Assert.All(parsed.Artifacts, artifact => Assert.NotNull(Address.Artifact(artifact.Path)));
        }
    }
}
