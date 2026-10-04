using System.IO;
using System.Reflection;
using DlFovFixer.App.Composition;

namespace DlFovFixer.App.Tests.Composition;

public sealed class BuildInfoTests
{
    [Fact]
    public void Current_MatchesTheStampedAttributes()
    {
        var assembly = typeof(BuildInfo).Assembly;
        var informational = assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()!.InformationalVersion;

        var build = BuildInfo.Current;

        Assert.Equal(informational, build.Version);
        Assert.Equal(!informational.EndsWith("-dev", StringComparison.Ordinal), build.IsRelease);
    }

    [Fact]
    public void Current_CarriesTheRepositorysManifestKey()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null && !File.Exists(Path.Combine(directory.FullName, "version.properties")))
        {
            directory = directory.Parent;
        }

        Assert.NotNull(directory);
        var key = File.ReadAllText(Path.Combine(directory.FullName, "contracts", "keys", "release-manifest-public.b64")).Trim();

        Assert.Equal(key, BuildInfo.Current.ManifestPublicKey);
    }
}
