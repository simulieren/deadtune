using System.Text.RegularExpressions;

namespace DlFovFixer.Infrastructure.Locating;

/// <summary>Reads the library folders a Steam install lists in <c>steamapps\libraryfolders.vdf</c>.</summary>
public static partial class SteamLibraries
{
    /// <summary>
    /// Every <c>"path"</c> value in the file's text, in order. The file doubles its backslashes, and
    /// they are halved again here.
    /// </summary>
    public static IReadOnlyList<string> PathsIn(string libraryFoldersText) =>
        [.. PathEntry().Matches(libraryFoldersText).Select(match => match.Groups[1].Value.Replace(@"\\", @"\", StringComparison.Ordinal))];

    [GeneratedRegex("\"path\"\\s*\"([^\"]+)\"", RegexOptions.CultureInvariant)]
    private static partial Regex PathEntry();
}
