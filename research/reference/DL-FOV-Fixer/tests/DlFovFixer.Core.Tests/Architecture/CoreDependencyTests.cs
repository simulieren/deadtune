using System.Reflection;

namespace DlFovFixer.Core.Tests.Architecture;

/// <summary>
/// Core holds the domain and the use cases. It must not reach for another layer, the UI, the
/// registry or the network (ARCHITECTURE.md, docs/csharp-rewrite.md).
/// </summary>
public sealed class CoreDependencyTests
{
    private static readonly string[] ForbiddenReferences =
    [
        "DlFovFixer.Infrastructure",
        "DlFovFixer.App",
        "PresentationCore",
        "PresentationFramework",
        "WindowsBase",
        "Microsoft.Win32.Registry",
        "System.Net.Http",
    ];

    [Fact]
    public void CoreAssembly_ReferencesNoOtherLayerUiRegistryOrNetwork()
    {
        var core = Assembly.Load("DlFovFixer.Core");

        var references = core.GetReferencedAssemblies().Select(name => name.Name);

        Assert.Empty(references.Intersect(ForbiddenReferences));
    }
}
