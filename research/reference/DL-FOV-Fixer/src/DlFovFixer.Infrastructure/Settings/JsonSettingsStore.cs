using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Settings;

namespace DlFovFixer.Infrastructure.Settings;

/// <summary>
/// Settings in the 1.0 config.json format, with the same key names, so an existing install keeps its
/// settings (ADR 0004). Reading forgives like 1.0 did: unknown keys are ignored, and a missing,
/// mistyped or corrupt value falls back to its default. Saving adds <c>schemaVersion</c>.
/// </summary>
public sealed class JsonSettingsStore(string path) : ISettingsStore
{
    public const int SchemaVersion = 2;

    private static readonly JsonSerializerOptions WriteOptions = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    /// <summary>Where 1.0 keeps its settings: <c>%APPDATA%\DL-FOV-Fixer\config.json</c>.</summary>
    public static string DefaultPath =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "DL-FOV-Fixer", "config.json");

    public bool Exists => File.Exists(path);

    public AppSettings Load()
    {
        JsonObject? root;
        try
        {
            root = JsonNode.Parse(File.ReadAllText(path)) as JsonObject;
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or JsonException)
        {
            root = null;
        }

        var defaults = AppSettings.Defaults;
        if (root is null)
        {
            return defaults;
        }

        return new AppSettings(
            GameInfoPath: ReadString(root, "gameinfo_path") ?? defaults.GameInfoPath,
            FovValue: ReadString(root, "fov_value") ?? defaults.FovValue,
            AutoApplyOnStart: ReadBool(root, "auto_apply_on_start") ?? defaults.AutoApplyOnStart,
            PeriodicCheckMinutes: ReadInt(root, "periodic_check_minutes") ?? defaults.PeriodicCheckMinutes,
            StartWithWindows: ReadBool(root, "start_with_windows") ?? defaults.StartWithWindows,
            CheckUpdatesOnStart: ReadBool(root, "check_updates_on_start") ?? defaults.CheckUpdatesOnStart,
            Tweaks: ReadTweaks(root["tweaks"] as JsonObject),
            ApplyTweaks: ReadBool(root, "apply_tweaks") ?? defaults.ApplyTweaks,
            Theme: ReadTheme(root) ?? defaults.Theme);
    }

    public void Save(AppSettings settings)
    {
        var root = new JsonObject
        {
            ["schemaVersion"] = SchemaVersion,
            ["gameinfo_path"] = settings.GameInfoPath,
            ["fov_value"] = settings.FovValue,
            ["auto_apply_on_start"] = settings.AutoApplyOnStart,
            ["periodic_check_minutes"] = settings.PeriodicCheckMinutes,
            ["start_with_windows"] = settings.StartWithWindows,
            ["check_updates_on_start"] = settings.CheckUpdatesOnStart,
            ["tweaks"] = new JsonObject
            {
                ["convars"] = WritePairs(settings.Tweaks.ConVars),
                ["scenesystem"] = WritePairs(settings.Tweaks.SceneSystem),
                ["video"] = WritePairs(settings.Tweaks.Video),
            },
            ["apply_tweaks"] = settings.ApplyTweaks,
            ["theme"] = settings.Theme.ToString().ToLowerInvariant(),
        };

        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        var temp = path + ".tmp";
        File.WriteAllText(temp, root.ToJsonString(WriteOptions) + Environment.NewLine);
        File.Move(temp, path, overwrite: true);
    }

    private static string? ReadString(JsonObject root, string key) => root[key] switch
    {
        JsonValue value when value.TryGetValue(out string? text) => text,

        // 1.0 turned whatever it found into text with str(), so a number stays usable.
        JsonValue value when value.GetValueKind() == JsonValueKind.Number => value.ToJsonString(),
        _ => null,
    };

    /// <summary>"system", "light" or "dark". 2.0 added the key, and 1.0 ignores it.</summary>
    private static ThemeMode? ReadTheme(JsonObject root) => ReadString(root, "theme") switch
    {
        "system" => ThemeMode.System,
        "light" => ThemeMode.Light,
        "dark" => ThemeMode.Dark,
        _ => null,
    };

    private static bool? ReadBool(JsonObject root, string key) =>
        root[key] is JsonValue value && value.TryGetValue(out bool flag) ? flag : null;

    private static int? ReadInt(JsonObject root, string key) =>
        root[key] is JsonValue value && value.TryGetValue(out int number) ? number : null;

    private static ExtraTweaks ReadTweaks(JsonObject? tweaks) =>
        tweaks is null
            ? ExtraTweaks.None
            : new ExtraTweaks(ReadPairs(tweaks["convars"]), ReadPairs(tweaks["scenesystem"]), ReadPairs(tweaks["video"]));

    /// <summary>Keeps every two-item list as a key and value, and drops anything else, as 1.0 did.</summary>
    private static TweakEntry[] ReadPairs(JsonNode? node) =>
        node is JsonArray items
            ? [.. items.OfType<JsonArray>().Where(pair => pair.Count == 2).Select(pair => new TweakEntry(PairText(pair[0]), PairText(pair[1])))]
            : [];

    /// <summary>The text 1.0's str() gave an item: strings as they are, other JSON values as Python prints them.</summary>
    private static string PairText(JsonNode? node) => node switch
    {
        null => "None",
        JsonValue value when value.TryGetValue(out string? text) => text,
        JsonValue value when value.TryGetValue(out bool flag) => flag ? "True" : "False",
        _ => node.ToJsonString(),
    };

    private static JsonArray WritePairs(IEnumerable<TweakEntry> entries) =>
        [.. entries.Select(entry => (JsonNode)new JsonArray(entry.Key, entry.Value))];
}
