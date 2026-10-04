namespace DlFovFixer.Core.Updates;

/// <summary>
/// Whether a published release is offered to the running app. Development builds never update
/// themselves and pre-releases are never offered (contracts/vectors/semantic-version.json).
/// </summary>
public static class UpdatePolicy
{
    public static bool ShouldOffer(string installed, string available)
    {
        var current = SemanticVersion.Parse(installed);
        var candidate = SemanticVersion.Parse(available);
        if (current is null || candidate is null || current.IsDevelopmentBuild || candidate.IsPrerelease)
        {
            return false;
        }

        return candidate > current;
    }
}
