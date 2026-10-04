using System.IO;
using System.Reflection;

namespace DlFovFixer.App.Tests.Versioning;

/// <summary>
/// Directory.Build.props stamps every build with the version in version.properties. Only a release
/// build carries the plain version, so the updater can never mistake a local build for a release.
/// </summary>
public sealed class BuildVersionTests
{
    private static readonly Assembly AppAssembly = typeof(DlFovFixer.App.App).Assembly;

    [Fact]
    public void AppAssembly_CarriesTheVersionFromVersionProperties()
    {
        var versionName = ReadVersionName();

        Assert.Equal(new Version(versionName + ".0"), AppAssembly.GetName().Version);
    }

    [Fact]
    public void AppAssembly_IsMarkedDevUnlessItIsAReleaseBuild()
    {
        var versionName = ReadVersionName();
        var expected = IsReleaseBuild() ? versionName : versionName + "-dev";

        var informational = AppAssembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>();

        Assert.Equal(expected, informational?.InformationalVersion);
    }

    private static bool IsReleaseBuild() =>
        AppAssembly.GetCustomAttributes<AssemblyMetadataAttribute>()
            .Single(attribute => attribute.Key == "DlFovFixer.ReleaseBuild")
            .Value == "true";

    private static string ReadVersionName()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null && !File.Exists(Path.Combine(directory.FullName, "version.properties")))
        {
            directory = directory.Parent;
        }

        Assert.NotNull(directory);
        var line = File.ReadLines(Path.Combine(directory.FullName, "version.properties"))
            .Single(text => text.StartsWith("versionName=", StringComparison.Ordinal));
        return line["versionName=".Length..].Trim();
    }
}
