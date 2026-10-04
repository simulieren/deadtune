using DlFovFixer.Infrastructure.Locating;
using DlFovFixer.Infrastructure.Tests.Support;
using Microsoft.Win32;

namespace DlFovFixer.Infrastructure.Tests.Locating;

/// <summary>Runs against a fake registry and a Steam tree in a temp folder, never the real ones.</summary>
public sealed class SteamGameInfoLocatorTests : IDisposable
{
    private readonly TempFolder _folder = new();

    public void Dispose() => _folder.Dispose();

    [Fact]
    public void Locate_DeadlockInTheSteamFolderFromTheRegistry_FindsIt()
    {
        var steam = _folder.File("Steam");
        var gameInfo = InstallDeadlock(steam);
        var registry = new FakeRegistry { [(RegistryHive.CurrentUser, "SteamPath")] = steam.Replace('\\', '/') };

        Assert.Equal(gameInfo, new SteamGameInfoLocator(registry, []).Locate());
    }

    [Fact]
    public void Locate_DeadlockInAnotherLibrary_FindsItThroughLibraryFolders()
    {
        var steam = _folder.File("Steam");
        var library = _folder.File("Games", "SteamLibrary");
        var gameInfo = InstallDeadlock(library);
        WriteLibraryFolders(steam, library);
        var registry = new FakeRegistry { [(RegistryHive.LocalMachine, "InstallPath")] = steam };

        Assert.Equal(gameInfo, new SteamGameInfoLocator(registry, []).Locate());
    }

    [Fact]
    public void Locate_NothingInTheRegistry_TriesTheCommonFoldersThatExist()
    {
        var common = _folder.File("SteamLibrary");
        var gameInfo = InstallDeadlock(common);

        var locator = new SteamGameInfoLocator(new FakeRegistry(), [_folder.File("does not exist"), common]);

        Assert.Equal(gameInfo, locator.Locate());
    }

    [Fact]
    public void Locate_NoDeadlockAnywhere_IsNull()
    {
        var steam = _folder.File("Steam");
        Directory.CreateDirectory(steam);
        var registry = new FakeRegistry { [(RegistryHive.CurrentUser, "SteamPath")] = steam };

        Assert.Null(new SteamGameInfoLocator(registry, [steam]).Locate());
    }

    [Fact]
    public void PathsIn_HalvesTheDoubledBackslashes()
    {
        const string vdf = """
            "libraryfolders"
            {
                "0" { "path"    "C:\\Program Files (x86)\\Steam" }
                "1"
                {
                    "path"		"D:\\SteamLibrary"
                    "apps" { "1422450" "123" }
                }
            }
            """;

        Assert.Equal([@"C:\Program Files (x86)\Steam", @"D:\SteamLibrary"], SteamLibraries.PathsIn(vdf));
    }

    [Theory]
    [InlineData("gameinfo.gi", "\"GameInfo\"\n{\n}\n", true)]
    [InlineData("GAMEINFO.GI", "\"GameInfo\"\n{\n}\n", true)]
    [InlineData("gameinfo.gi", "\"SomethingElse\"\n{\n}\n", false)]
    [InlineData("gameinfo.txt", "\"GameInfo\"\n{\n}\n", false)]
    public void LooksLikeGameInfo_ChecksTheNameAndTheHeader(string name, string content, bool expected)
    {
        var path = _folder.WriteBytes(name, System.Text.Encoding.UTF8.GetBytes(content));

        Assert.Equal(expected, new SteamGameInfoLocator(new FakeRegistry(), []).LooksLikeGameInfo(path));
    }

    private static string InstallDeadlock(string library)
    {
        var gameInfo = Path.Combine(library, "steamapps", "common", "Deadlock", "game", "citadel", "gameinfo.gi");
        Directory.CreateDirectory(Path.GetDirectoryName(gameInfo)!);
        File.WriteAllText(gameInfo, "\"GameInfo\"\n{\n}\n");
        return gameInfo;
    }

    private static void WriteLibraryFolders(string steam, params string[] libraries)
    {
        var steamApps = Path.Combine(steam, "steamapps");
        Directory.CreateDirectory(steamApps);
        var entries = libraries.Select((library, index) => $"\t\"{index}\"\n\t{{\n\t\t\"path\"\t\t\"{library.Replace(@"\", @"\\", StringComparison.Ordinal)}\"\n\t}}\n");
        File.WriteAllText(Path.Combine(steamApps, "libraryfolders.vdf"), $"\"libraryfolders\"\n{{\n{string.Concat(entries)}}}\n");
    }

    /// <summary>Answers by hive and value name, whatever the key path.</summary>
    private sealed class FakeRegistry : Dictionary<(RegistryHive Hive, string Value), string>, IRegistryReader
    {
        public string? ReadString(RegistryHive hive, string keyPath, string valueName) =>
            TryGetValue((hive, valueName), out var value) ? value : null;
    }
}
