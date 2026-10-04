using System.Text.Json;
using System.Text.RegularExpressions;

namespace DlFovFixer.Core.Updates;

/// <summary>
/// Reads and checks manifest.json. Unknown properties are ignored, and every rule is covered by
/// contracts/vectors/release-channel.json.
/// </summary>
public static partial class ReleaseManifestParser
{
    public const int SupportedSchema = 1;

    /// <summary>The largest artifact the app agrees to download.</summary>
    public const long MaxArtifactSize = 524_288_000;

    public static ManifestCheck Parse(ReadOnlyMemory<byte> bytes)
    {
        JsonDocument document;
        try
        {
            document = JsonDocument.Parse(bytes);
        }
        catch (JsonException)
        {
            return new ManifestCheck.BadManifest("not JSON");
        }

        using (document)
        {
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
            {
                return new ManifestCheck.BadManifest("not a JSON object");
            }

            if (Integer(root, "schema") != SupportedSchema)
            {
                return new ManifestCheck.BadManifest("unsupported schema");
            }

            var versionText = Text(root, "version");
            if (SemanticVersion.Parse(versionText) is not { } version)
            {
                return new ManifestCheck.BadManifest($"version is not semantic: {versionText}");
            }

            if (Text(root, "publishedAt") is not { } publishedAt)
            {
                return new ManifestCheck.BadManifest("publishedAt missing");
            }

            if (!root.TryGetProperty("artifacts", out var artifactsJson)
                || artifactsJson.ValueKind != JsonValueKind.Array
                || artifactsJson.GetArrayLength() == 0)
            {
                return new ManifestCheck.BadManifest("no artifacts");
            }

            var artifacts = new List<ReleaseArtifact>();
            foreach (var element in artifactsJson.EnumerateArray())
            {
                if (ParseArtifact(element, version) is not { } artifact)
                {
                    return new ManifestCheck.BadManifest($"invalid artifact {element.GetRawText()}");
                }

                artifacts.Add(artifact);
            }

            return new ManifestCheck.Valid(new ReleaseManifest(version, publishedAt, Text(root, "notes"), artifacts));
        }
    }

    private static ReleaseArtifact? ParseArtifact(JsonElement element, SemanticVersion version)
    {
        if (element.ValueKind != JsonValueKind.Object
            || Text(element, "kind") is not { } kind
            || Text(element, "path") is not { } path
            || Text(element, "sha256") is not { } sha256
            || Integer(element, "size") is not { } size)
        {
            return null;
        }

        // An artifact must sit on this very release, so a manifest cannot point at an older one.
        var file = path.StartsWith(version + "/", StringComparison.Ordinal) ? path[(version.ToString().Length + 1)..] : null;
        var valid = KindPattern().IsMatch(kind)
            && file is not null and not "." and not ".."
            && FilePattern().IsMatch(file)
            && size is >= 1 and <= MaxArtifactSize
            && Sha256Pattern().IsMatch(sha256);
        return valid ? new ReleaseArtifact(kind, path, size, sha256) : null;
    }

    private static string? Text(JsonElement element, string name) =>
        element.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() : null;

    private static long? Integer(JsonElement element, string name) =>
        element.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.Number && value.TryGetInt64(out var number)
            ? number
            : null;

    // \z, not $: in .NET, $ also matches before a trailing newline.
    [GeneratedRegex(@"^[a-z]+\z", RegexOptions.CultureInvariant)]
    private static partial Regex KindPattern();

    [GeneratedRegex(@"^[A-Za-z0-9._-]+\z", RegexOptions.CultureInvariant)]
    private static partial Regex FilePattern();

    [GeneratedRegex(@"^[0-9a-f]{64}\z", RegexOptions.CultureInvariant)]
    private static partial Regex Sha256Pattern();
}
