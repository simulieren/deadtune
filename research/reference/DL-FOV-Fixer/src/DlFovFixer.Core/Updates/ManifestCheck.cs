namespace DlFovFixer.Core.Updates;

/// <summary>The outcome of reading manifest.json.</summary>
public abstract record ManifestCheck
{
    private ManifestCheck()
    {
    }

    public sealed record Valid(ReleaseManifest Manifest) : ManifestCheck;

    public sealed record BadManifest(string Reason) : ManifestCheck;
}
