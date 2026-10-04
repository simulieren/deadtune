using System.Text.Json.Nodes;
using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Settings;
using DlFovFixer.Infrastructure.Settings;
using DlFovFixer.Infrastructure.Tests.Support;

namespace DlFovFixer.Infrastructure.Tests.Settings;

public sealed class JsonSettingsStoreTests : IDisposable
{
    private readonly TempFolder _folder = new();

    public void Dispose() => _folder.Dispose();

    [Fact]
    public void Load_AConfigWrittenBy100_KeepsEverySetting()
    {
        var path = _folder.File("config.json");
        File.Copy(Path.Combine(AppContext.BaseDirectory, "Fixtures", "config-1.0.0.json"), path);

        var settings = new JsonSettingsStore(path).Load();

        Assert.Equal(@"D:\Games\SteamLibrary\steamapps\common\Deadlock\game\citadel\gameinfo.gi", settings.GameInfoPath);
        Assert.Equal("2.49", settings.FovValue);
        Assert.False(settings.AutoApplyOnStart);
        Assert.Equal(5, settings.PeriodicCheckMinutes);
        Assert.True(settings.StartWithWindows);
        Assert.False(settings.CheckUpdatesOnStart);
        Assert.False(settings.ApplyTweaks);
        Assert.Equal(["r_directlighting=0", "cl_new=1"], Pairs(settings.Tweaks.ConVars));
        Assert.Equal(["VolumetricFog=0"], Pairs(settings.Tweaks.SceneSystem));
        Assert.Equal(["setting.fps_max=240"], Pairs(settings.Tweaks.Video));
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("{ not json")]
    [InlineData("[1, 2]")]
    [InlineData("{}")]
    public void Load_MissingEmptyOrCorruptFile_GivesTheDefaults(string? content)
    {
        var path = _folder.File("config.json");
        if (content is not null)
        {
            File.WriteAllText(path, content);
        }

        var settings = new JsonSettingsStore(path).Load();

        AssertSame(AppSettings.Defaults, settings);
    }

    [Fact]
    public void Load_MistypedOrUnknownValues_FallBackPerSetting()
    {
        var path = _folder.File("config.json");
        File.WriteAllText(path, """
            {
              "fov_value": 2.15,
              "auto_apply_on_start": "yes",
              "periodic_check_minutes": "ten",
              "some_future_key": true,
              "tweaks": { "convars": [["a", "1"], ["too", "many", "items"], "not a pair", [2, true]], "video": "nope" }
            }
            """);

        var settings = new JsonSettingsStore(path).Load();

        Assert.Equal("2.15", settings.FovValue);
        Assert.True(settings.AutoApplyOnStart);
        Assert.Equal(10, settings.PeriodicCheckMinutes);
        Assert.Equal(["a=1", "2=True"], Pairs(settings.Tweaks.ConVars));
        Assert.Empty(settings.Tweaks.Video);
    }

    [Fact]
    public void Exists_IsFalseUntilTheFirstSave()
    {
        var store = new JsonSettingsStore(_folder.File("config.json"));
        Assert.False(store.Exists);

        store.Save(AppSettings.Defaults);

        Assert.True(store.Exists);
    }

    [Fact]
    public void SaveThenLoad_RoundTripsEverySetting()
    {
        var store = new JsonSettingsStore(_folder.File("nested", "config.json"));
        var saved = new AppSettings(
            GameInfoPath: @"E:\Steam\steamapps\common\Deadlock\game\citadel\gameinfo.gi",
            FovValue: "3.00",
            AutoApplyOnStart: false,
            PeriodicCheckMinutes: 0,
            StartWithWindows: true,
            CheckUpdatesOnStart: false,
            Tweaks: new ExtraTweaks([new("cl_x", "two words")], [new("Csm", "0")], [new("setting.a", "1")]),
            ApplyTweaks: false,
            Theme: ThemeMode.Dark);

        store.Save(saved);

        AssertSame(saved, store.Load());
    }

    [Fact]
    public void Save_Keeps10KeyNamesAndAddsTheSchemaVersionAndTheTheme()
    {
        var path = _folder.File("config.json");

        new JsonSettingsStore(path).Save(AppSettings.Defaults);

        var root = JsonNode.Parse(File.ReadAllText(path))!.AsObject();
        Assert.Equal(
            ["schemaVersion", "gameinfo_path", "fov_value", "auto_apply_on_start", "periodic_check_minutes", "start_with_windows", "check_updates_on_start", "tweaks", "apply_tweaks", "theme"],
            root.Select(property => property.Key));
        Assert.Equal(JsonSettingsStore.SchemaVersion, root["schemaVersion"]!.GetValue<int>());
        Assert.Equal("system", root["theme"]!.GetValue<string>());
        Assert.Equal(["convars", "scenesystem", "video"], root["tweaks"]!.AsObject().Select(property => property.Key));
        Assert.Equal(["config.json"], Directory.GetFiles(_folder.Path).Select(Path.GetFileName));
    }

    [Theory]
    [InlineData("\"light\"", ThemeMode.Light)]
    [InlineData("\"dark\"", ThemeMode.Dark)]
    [InlineData("\"system\"", ThemeMode.System)]
    [InlineData("\"Dark\"", ThemeMode.System)]
    [InlineData("\"purple\"", ThemeMode.System)]
    [InlineData("1", ThemeMode.System)]
    public void Load_Theme_FallsBackToSystemUnlessItIsKnown(string json, ThemeMode expected)
    {
        var path = _folder.File("config.json");
        File.WriteAllText(path, $"{{ \"theme\": {json} }}");

        Assert.Equal(expected, new JsonSettingsStore(path).Load().Theme);
    }

    private static string[] Pairs(IEnumerable<TweakEntry> entries) => [.. entries.Select(entry => $"{entry.Key}={entry.Value}")];

    private static void AssertSame(AppSettings expected, AppSettings actual)
    {
        Assert.Equal(expected with { Tweaks = ExtraTweaks.None }, actual with { Tweaks = ExtraTweaks.None });
        Assert.Equal(Pairs(expected.Tweaks.ConVars), Pairs(actual.Tweaks.ConVars));
        Assert.Equal(Pairs(expected.Tweaks.SceneSystem), Pairs(actual.Tweaks.SceneSystem));
        Assert.Equal(Pairs(expected.Tweaks.Video), Pairs(actual.Tweaks.Video));
    }
}
