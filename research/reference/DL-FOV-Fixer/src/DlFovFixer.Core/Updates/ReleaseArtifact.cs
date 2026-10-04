namespace DlFovFixer.Core.Updates;

/// <summary>
/// One file of a release, as the manifest lists it. <see cref="Path"/> is "X.Y.Z/file", the asset on
/// the release tagged vX.Y.Z.
/// </summary>
public sealed record ReleaseArtifact(string Kind, string Path, long Size, string Sha256)
{
    /// <summary>The kind of the per-user installer, the only artifact 2.0 installs.</summary>
    public const string InstallerKind = "installer";

    /// <summary>The file name, which is the part after the version.</summary>
    public string FileName => Path[(Path.IndexOf('/', StringComparison.Ordinal) + 1)..];
}
