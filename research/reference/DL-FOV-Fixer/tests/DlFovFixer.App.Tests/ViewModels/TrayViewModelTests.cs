using DlFovFixer.App.Tests.Support;
using DlFovFixer.App.ViewModels;
using DlFovFixer.Core.Applying;
using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Settings;
using static DlFovFixer.App.Tests.Support.GameInfoSamples;

namespace DlFovFixer.App.Tests.ViewModels;

public sealed class TrayViewModelTests
{
    private readonly FakeGameFiles _files = new();
    private readonly FakeLocator _locator = new();
    private readonly FakeShell _shell = new();
    private FakeSettingsStore _store = new(AppSettings.Defaults with { GameInfoPath = GameInfoPath, FovValue = "2.49" });

    [Fact]
    public void Start_FirstRun_FindsTheFileAndKeepsTheValueAlreadyInIt()
    {
        _store = new FakeSettingsStore();
        _locator.Found = GameInfoPath;
        _files.Texts[GameInfoPath] = GameInfoWith249;

        var model = Create();
        model.Start();

        Assert.Equal(GameInfoPath, _store.Saved!.GameInfoPath);
        Assert.Equal("2.49", _store.Saved.FovValue);
        Assert.Equal(FixState.Ok, model.Status.State);
        Assert.Empty(_files.Writes);
        Assert.Contains(_shell.Notifications, message => message.Contains("running in the tray", StringComparison.Ordinal));
    }

    [Fact]
    public void Start_FirstRunNothingFound_AsksForTheFileOnceAndSavesTheDefaults()
    {
        _store = new FakeSettingsStore();

        var model = Create();
        model.Start();

        Assert.Equal(1, _shell.PathRequests);
        Assert.Equal(AppSettings.Defaults, _store.Saved);
        Assert.Equal(FixState.Missing, model.Status.State);
    }

    [Fact]
    public void Start_AutoApplyOn_AppliesWithoutAsking()
    {
        _files.Texts[GameInfoPath] = GameInfoWithout;

        var model = Create();
        model.Start();

        Assert.Equal([GameInfoPath], _files.Writes);
        Assert.Equal(FixState.Ok, model.Status.State);
        Assert.Equal(0, _shell.PathRequests);
        Assert.StartsWith("Applied: FOV 2.49 (~101°)", _shell.Notifications.Single(), StringComparison.Ordinal);
    }

    [Fact]
    public void Start_AutoApplyOff_OnlyReadsTheFile()
    {
        _store = new FakeSettingsStore(_store.Saved! with { AutoApplyOnStart = false });
        _files.Texts[GameInfoPath] = GameInfoWithout;

        var model = Create();
        model.Start();

        Assert.Empty(_files.Writes);
        Assert.Equal(FixState.NotApplied, model.Status.State);
    }

    [Fact]
    public void Tick_AfterAGameUpdate_ReappliesAndSaysSo()
    {
        _files.Texts[GameInfoPath] = GameInfoWith249;
        var model = Create();
        model.Start();
        _shell.Notifications.Clear();
        _files.Texts[GameInfoPath] = GameInfoWithout;

        model.Tick();

        Assert.Equal(FixState.Ok, model.Status.State);
        Assert.Contains("Re-applied", _shell.Notifications.Single(), StringComparison.Ordinal);
    }

    [Fact]
    public void Tick_NothingChanged_StaysQuiet()
    {
        _files.Texts[GameInfoPath] = GameInfoWith249;
        var model = Create();
        model.Start();
        _shell.Notifications.Clear();

        model.Tick();

        Assert.Empty(_shell.Notifications);
        Assert.Empty(_files.Writes);
    }

    [Fact]
    public void Tick_GameHasTheFileOpen_WaitsAndDoesNotTurnRed()
    {
        _files.Texts[GameInfoPath] = GameInfoWithout;
        _files.WriteFailure = new GameFileException("in use", isLocked: true);
        var model = Create();

        model.Tick();

        Assert.Equal(FixState.Waiting, model.Status.State);
        Assert.Empty(_shell.Notifications);
    }

    [Fact]
    public void ApplyNow_WriteFails_StaysFailedUntilTheNextApply()
    {
        _files.Texts[GameInfoPath] = GameInfoWithout;
        _files.WriteFailure = new GameFileException("disk full", isLocked: false);
        var model = Create();

        model.ApplyNow();
        model.ToggleApplyTweaks();

        Assert.Equal(FixState.Failed, model.Status.State);
        Assert.Contains("disk full", _shell.Notifications.Single(), StringComparison.Ordinal);

        _files.WriteFailure = null;
        model.ApplyNow();

        Assert.Equal(FixState.Ok, model.Status.State);
    }

    [Fact]
    public void ApplyNow_NoFileAnywhere_AsksAndSaysWhereToLookWhenCancelled()
    {
        var model = Create();

        model.ApplyNow();

        Assert.Equal(1, _shell.PathRequests);
        Assert.Contains("Locate gameinfo.gi", _shell.Notifications.Single(), StringComparison.Ordinal);
        Assert.Equal(FixState.Missing, model.Status.State);
    }

    [Fact]
    public void ApplyNow_PickedFileIsNotGameInfo_SaysSoAndKeepsThePath()
    {
        _shell.PickedPath = @"D:\Downloads\notes.txt";
        var model = Create();

        model.ApplyNow();

        Assert.Contains("doesn't look like", _shell.Messages.Single(), StringComparison.Ordinal);
        Assert.Equal(GameInfoPath, model.Settings.GameInfoPath);
    }

    [Theory]
    [InlineData("2,66", "2.66")]
    [InlineData(" 3 ", "3")]
    public void SetValue_ANumber_SavesItNormalizedAndApplies(string typed, string saved)
    {
        _files.Texts[GameInfoPath] = GameInfoWith249;
        var model = Create();

        model.SetValue(typed);

        Assert.Equal(saved, _store.Saved!.FovValue);
        Assert.Contains($"\"r_aspectratio\" \"{saved}\"", _files.Texts[GameInfoPath], StringComparison.Ordinal);
    }

    [Theory]
    [InlineData("wide")]
    [InlineData("9")]
    [InlineData("0_6")]
    public void SetValue_NotAValue_ChangesNothing(string typed)
    {
        _files.Texts[GameInfoPath] = GameInfoWith249;
        var model = Create();

        model.SetValue(typed);

        Assert.Equal("2.49", _store.Saved!.FovValue);
        Assert.Empty(_files.Writes);
        Assert.Contains("between 0.5 and 6.0", _shell.Notifications.Single(), StringComparison.Ordinal);
    }

    [Fact]
    public void ChooseCustomValue_Cancelled_ChangesNothing()
    {
        var model = Create();

        model.ChooseCustomValue();

        Assert.Empty(_shell.Notifications);
        Assert.Equal("2.49", _store.Saved!.FovValue);
    }

    [Fact]
    public void CheckNow_ValueDrifted_SaysBothValuesAndWritesNothing()
    {
        _files.Texts[GameInfoPath] = GameInfoWith249.Replace("2.49", "2.3", StringComparison.Ordinal);
        var model = Create();

        model.CheckNow();

        Assert.Equal(FixState.Drifted, model.Status.State);
        Assert.Equal("The file has 2.3 (~95°), and your target is 2.49 (~101°). Click 'Apply now'.", _shell.Notifications.Single());
        Assert.Empty(_files.Writes);
    }

    [Fact]
    public void ImportTweaks_MergesIntoTheStoredOnesAndTakesThePastedValue()
    {
        _store = new FakeSettingsStore(_store.Saved! with
        {
            Tweaks = ExtraTweaks.None with { ConVars = [new("r_directlighting", "1")] },
        });
        _files.Texts[GameInfoPath] = GameInfoWith249;
        _shell.PastedConfig = "\"r_aspectratio\" \"2.15\"\n\"r_directlighting\" \"0\"\nVolumetricFog 0\nsetting.fps_max 240\n";
        var model = Create();

        model.ImportTweaks();

        var saved = _store.Saved!;
        Assert.Equal("2.15", saved.FovValue);
        Assert.Equal([new TweakEntry("r_directlighting", "0")], saved.Tweaks.ConVars);
        Assert.Equal([new TweakEntry("VolumetricFog", "0")], saved.Tweaks.SceneSystem);
        Assert.Equal([new TweakEntry("setting.fps_max", "240")], saved.Tweaks.Video);
        Assert.StartsWith("Imported 1 convars, 1 scenesystem, 1 video. FOV set to 2.15 (~91°).", _shell.Notifications[0], StringComparison.Ordinal);
        Assert.Equal("Applied: FOV 2.15 (~91°) · 1 convars + 1 scene + 1 video (created).", _shell.Notifications[1]);
    }

    [Fact]
    public void ClearTweaks_NotConfirmed_KeepsThem()
    {
        var tweaks = ExtraTweaks.None with { Video = [new("setting.fps_max", "240")] };
        _store = new FakeSettingsStore(_store.Saved! with { Tweaks = tweaks });
        var model = Create();

        model.ClearTweaks();
        Assert.Equal(tweaks, _store.Saved!.Tweaks);

        _shell.Confirms = true;
        model.ClearTweaks();
        Assert.Equal(ExtraTweaks.None, _store.Saved.Tweaks);
    }

    [Fact]
    public void ViewTweaks_ListsEachDestination()
    {
        _store = new FakeSettingsStore(_store.Saved! with
        {
            Tweaks = ExtraTweaks.None with { SceneSystem = [new("VolumetricFog", "0")] },
        });
        var model = Create();

        model.ViewTweaks();

        var text = _shell.Shown!.Value.Text;
        Assert.Contains("[SceneSystem]  (1)", text, StringComparison.Ordinal);
        Assert.Contains("    VolumetricFog  0", text, StringComparison.Ordinal);
        Assert.Contains("[video.cfg]  (0)", text, StringComparison.Ordinal);
    }

    [Fact]
    public void ToggleSignInStartup_FlipsTheEntryAndStoresIt()
    {
        var model = Create();

        model.ToggleSignInStartup();

        Assert.True(model.IsSignInStartupEnabled);
        Assert.True(_store.Saved!.StartWithWindows);
    }

    [Fact]
    public void LocateGameInfo_APickedFile_IsSavedAndClearsAFailure()
    {
        _files.Texts[GameInfoPath] = GameInfoWithout;
        _files.WriteFailure = new GameFileException("denied", isLocked: false);
        var model = Create();
        model.ApplyNow();
        var other = @"E:\SteamLibrary\steamapps\common\Deadlock\game\citadel\gameinfo.gi";
        _files.Texts[other] = GameInfoWith249;
        _shell.PickedPath = other;

        model.LocateGameInfo();

        Assert.Equal(other, _store.Saved!.GameInfoPath);
        Assert.Equal(FixState.Ok, model.Status.State);
    }

    [Fact]
    public void OpenGameInfo_OpensTheStoredFile()
    {
        _files.Texts[GameInfoPath] = GameInfoWith249;
        var model = Create();

        model.OpenGameInfo();

        Assert.Equal([GameInfoPath], _shell.Opened);
    }

    [Fact]
    public void Changed_FiresWhenASettingChanges()
    {
        var model = Create();
        var fired = 0;
        model.Changed += (_, _) => fired++;

        model.ToggleAutoApply();

        Assert.Equal(1, fired);
        Assert.False(_store.Saved!.AutoApplyOnStart);
    }

    [Fact]
    public void SetTheme_SavesItAndSaysSoOnlyWhenItChanges()
    {
        var model = Create();
        var fired = 0;
        model.Changed += (_, _) => fired++;

        model.SetTheme(ThemeMode.Dark);
        model.SetTheme(ThemeMode.Dark);

        Assert.Equal(ThemeMode.Dark, _store.Saved!.Theme);
        Assert.Equal(1, fired);
    }

    [Theory]
    [InlineData(FixState.Ok, "DL FOV Fixer: up to date")]
    [InlineData(FixState.Waiting, "DL FOV Fixer: waiting for Deadlock to close")]
    [InlineData(FixState.Failed, "DL FOV Fixer: last update failed")]
    public void Describe_NamesEveryState(FixState state, string line) =>
        Assert.Equal(line, $"{TrayViewModel.AppTitle}: {TrayViewModel.Describe(state)}");

    private TrayViewModel Create()
    {
        var files = _files;
        return new TrayViewModel(_store, new ApplyService(files), new StatusProbe(files), _locator, _shell, files, _shell, _shell, _shell);
    }
}
