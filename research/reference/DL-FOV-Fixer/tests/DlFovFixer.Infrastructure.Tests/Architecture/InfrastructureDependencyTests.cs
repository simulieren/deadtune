using System.Reflection;

namespace DlFovFixer.Infrastructure.Tests.Architecture;

/// <summary>
/// Infrastructure implements Core's ports. It must not depend on the shell or on WPF, so the
/// adapters stay testable without a UI thread.
/// </summary>
public sealed class InfrastructureDependencyTests
{
    private static readonly string[] ForbiddenReferences =
    [
        "DlFovFixer.App",
        "PresentationCore",
        "PresentationFramework",
    ];

    [Fact]
    public void InfrastructureAssembly_ReferencesNeitherTheAppNorWpf()
    {
        var infrastructure = Assembly.Load("DlFovFixer.Infrastructure");

        var references = infrastructure.GetReferencedAssemblies().Select(name => name.Name);

        Assert.Empty(references.Intersect(ForbiddenReferences));
    }
}
