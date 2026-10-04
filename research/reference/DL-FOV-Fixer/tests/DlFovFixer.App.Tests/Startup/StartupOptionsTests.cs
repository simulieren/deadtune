using System.IO;
using DlFovFixer.App.Startup;

namespace DlFovFixer.App.Tests.Startup;

public sealed class StartupOptionsTests
{
    [Fact]
    public void Parse_NoArguments_UsesTheRealSettings() =>
        Assert.Null(StartupOptions.Parse([]).SettingsPath);

    [Fact]
    public void Parse_Settings_TakesTheFullPath()
    {
        var options = StartupOptions.Parse(["--SETTINGS", @"C:\Temp\test.json"]);

        Assert.Equal(@"C:\Temp\test.json", options.SettingsPath);
    }

    [Fact]
    public void Parse_SettingsRelative_IsResolvedAgainstTheWorkingFolder()
    {
        var options = StartupOptions.Parse(["--settings", "test.json"]);

        Assert.Equal(Path.GetFullPath("test.json"), options.SettingsPath);
    }

    [Theory]
    [InlineData("--settings")]
    [InlineData("--unknown")]
    public void Parse_IncompleteOrUnknown_IsIgnored(string argument) =>
        Assert.Null(StartupOptions.Parse([argument]).SettingsPath);
}
