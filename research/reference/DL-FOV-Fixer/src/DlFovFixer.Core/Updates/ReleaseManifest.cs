namespace DlFovFixer.Core.Updates;

/// <summary>The latest release as its manifest.json describes it.</summary>
public sealed record ReleaseManifest(SemanticVersion Version, string PublishedAt, string? Notes, IReadOnlyList<ReleaseArtifact> Artifacts)
{
    public ReleaseArtifact? Installer =>
        Artifacts.FirstOrDefault(artifact => artifact.Kind == ReleaseArtifact.InstallerKind);
}
