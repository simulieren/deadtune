using System.IO;

namespace DlFovFixer.App.Startup;

/// <summary>
/// The command line. <c>--settings &lt;path&gt;</c> uses another config.json, so a test run never
/// touches the real settings or, through them, the real game files. Anything else is ignored.
/// </summary>
public sealed record StartupOptions(string? SettingsPath)
{
    public static StartupOptions Parse(IReadOnlyList<string> arguments)
    {
        string? settingsPath = null;
        for (var i = 0; i < arguments.Count; i++)
        {
            if (string.Equals(arguments[i], "--settings", StringComparison.OrdinalIgnoreCase) && i + 1 < arguments.Count)
            {
                settingsPath = Path.GetFullPath(arguments[++i]);
            }
        }

        return new StartupOptions(settingsPath);
    }
}
