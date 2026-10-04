using System.Text;
using DlFovFixer.Core.Locating;
using Microsoft.Win32;

namespace DlFovFixer.Infrastructure.Locating;

/// <summary>
/// Finds gameinfo.gi in a Steam install: the Steam folder from the registry, then the common install
/// folders that exist, and for each of them every library its libraryfolders.vdf lists.
/// </summary>
public sealed class SteamGameInfoLocator(IRegistryReader registry, IReadOnlyList<string> commonSteamFolders) : IGameInfoLocator
{
    /// <summary>Where Steam or a Steam library usually sits when the registry does not say.</summary>
    public static IReadOnlyList<string> CommonSteamFolders { get; } =
    [
        @"C:\Program Files (x86)\Steam",
        @"C:\Program Files\Steam",
        @"D:\Steam",
        @"D:\SteamLibrary",
        @"E:\Steam",
        @"E:\SteamLibrary",
    ];

    private static readonly string GameInfoInLibrary =
        Path.Combine("steamapps", "common", "Deadlock", "game", "citadel", "gameinfo.gi");

    private static readonly (RegistryHive Hive, string Key, string Value)[] SteamPathValues =
    [
        (RegistryHive.CurrentUser, @"Software\Valve\Steam", "SteamPath"),
        (RegistryHive.LocalMachine, @"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (RegistryHive.LocalMachine, @"SOFTWARE\Valve\Steam", "InstallPath"),
    ];

    public string? Locate()
    {
        foreach (var steam in SteamFolders())
        {
            foreach (var library in LibrariesOf(steam))
            {
                var candidate = Path.Combine(library, GameInfoInLibrary);
                if (File.Exists(candidate))
                {
                    return candidate;
                }
            }
        }

        return null;
    }

    public bool LooksLikeGameInfo(string path)
    {
        if (string.IsNullOrEmpty(path) || !string.Equals(Path.GetFileName(path), "gameinfo.gi", StringComparison.OrdinalIgnoreCase))
        {
            return false;
        }

        try
        {
            using var reader = new StreamReader(path, Encoding.UTF8);
            var head = new char[4096];
            var count = reader.ReadBlock(head, 0, head.Length);
            return new string(head, 0, count).Contains("GameInfo", StringComparison.Ordinal);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            return false;
        }
    }

    private IEnumerable<string> SteamFolders()
    {
        var folders = new List<string>();
        var fromRegistry = SteamPathValues
            .Select(entry => registry.ReadString(entry.Hive, entry.Key, entry.Value))
            .FirstOrDefault(value => !string.IsNullOrEmpty(value));
        if (fromRegistry is not null)
        {
            folders.Add(Normalize(fromRegistry));
        }

        folders.AddRange(commonSteamFolders.Where(Directory.Exists).Select(Normalize));
        return folders.Distinct(StringComparer.OrdinalIgnoreCase);
    }

    private static IEnumerable<string> LibrariesOf(string steam)
    {
        var libraries = new List<string> { steam };
        try
        {
            var text = File.ReadAllText(Path.Combine(steam, "steamapps", "libraryfolders.vdf"));
            libraries.AddRange(SteamLibraries.PathsIn(text).Select(Normalize));
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            // No library list means the Steam folder is the only library.
        }

        return libraries.Distinct(StringComparer.OrdinalIgnoreCase);
    }

    /// <summary>The registry keeps SteamPath with forward slashes, so paths are normalized before comparing.</summary>
    private static string Normalize(string path) => Path.GetFullPath(path).TrimEnd(Path.DirectorySeparatorChar);
}
