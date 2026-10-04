using System.IO;
using DlFovFixer.App.Shell;
using DlFovFixer.Core.Applying;
using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Locating;
using DlFovFixer.Core.Settings;
using DlFovFixer.Core.Startup;

namespace DlFovFixer.App.ViewModels;

/// <summary>
/// Everything the tray menu does, without WPF: the menu reads its labels here and calls one method
/// per item. <see cref="Changed"/> fires whenever the status or a setting may have changed.
/// </summary>
public sealed class TrayViewModel(
    ISettingsStore store,
    ApplyService applier,
    StatusProbe probe,
    IGameInfoLocator locator,
    ISignInStartup signInStartup,
    IGameFiles files,
    IUserPrompts prompts,
    INotifier notifier,
    IFileOpener opener)
{
    public const string AppTitle = "DL FOV Fixer";

    private const string NotLocated = "Couldn't find gameinfo.gi. Use 'Locate gameinfo.gi…'.";
    private const string NotGameInfo = "That doesn't look like a Deadlock gameinfo.gi file.";

    // A lock or a failure an apply ran into. It stands until the next apply or check, because
    // reading the file again may well succeed while writing it still would not.
    private FixStatus? _applyProblem;

    public event EventHandler? Changed;

    public AppSettings Settings { get; private set; } = store.Load();

    public FixStatus Status { get; private set; } = new(FixState.Missing);

    public bool IsSignInStartupEnabled => signInStartup.IsEnabled;

    public string StatusLine => $"{AppTitle}: {Describe(Status.State)}";

    public string TargetLine => $"Target: r_aspectratio {FovLabel(Settings.FovValue)}";

    public string StoredTweaksLine => $"Stored: {Counts(Settings.Tweaks)}";

    /// <summary>The first-run setup, then the apply on start when it is on.</summary>
    public void Start()
    {
        if (!store.Exists)
        {
            SetUpFirstRun();
        }

        if (Settings.AutoApplyOnStart)
        {
            Apply(interactive: false, notify: true);
        }
        else
        {
            Refresh();
        }
    }

    /// <summary>The periodic check. It re-applies quietly and speaks only when a game update undid the fix.</summary>
    public void Tick()
    {
        if (Settings.AutoApplyOnStart && HasGameInfo())
        {
            if (RunApply().Result == ApplyResult.Applied)
            {
                notifier.Notify($"Re-applied your config after a game change. FOV {FovLabel(Settings.FovValue)}.");
            }
        }
        else
        {
            Refresh();
        }
    }

    public void ApplyNow() => Apply(interactive: true, notify: true);

    public void CheckNow()
    {
        if (!EnsureLocated(interactive: true))
        {
            notifier.Notify(NotLocated);
            Refresh();
            return;
        }

        _applyProblem = null;
        Refresh();
        var target = Settings.FovValue;
        notifier.Notify(Status switch
        {
            { State: FixState.NotApplied } => "r_aspectratio is not set in the file. Click 'Apply now'.",
            { State: FixState.Ok } => $"The file is up to date: r_aspectratio {FovLabel(target)}.",
            { State: FixState.Drifted, CurrentValue: { } current } =>
                $"The file has {FovLabel(current)}, and your target is {FovLabel(target)}. Click 'Apply now'.",
            { State: FixState.Waiting } => "The file is in use. Is Deadlock running?",
            _ => $"Couldn't read gameinfo.gi: {Status.Error}",
        });
    }

    public void ChooseCustomValue()
    {
        if (prompts.AskValue(Settings.FovValue) is { } value)
        {
            SetValue(value);
        }
    }

    public void SetValue(string value)
    {
        if (AspectRatio.Normalize(value) is not { } normalized)
        {
            notifier.Notify("Please enter a number between 0.5 and 6.0, like 2.15.");
            return;
        }

        Save(Settings with { FovValue = normalized });
        Apply(interactive: true, notify: true);
    }

    public void ImportTweaks()
    {
        var text = prompts.AskPastedConfig();
        if (string.IsNullOrWhiteSpace(text))
        {
            return;
        }

        var parsed = TweakTextParser.Parse(text);
        var stored = Settings.Tweaks;
        var settings = Settings with
        {
            Tweaks = new ExtraTweaks(
                TweakTextParser.MergeLists(stored.ConVars, parsed.ConVars),
                TweakTextParser.MergeLists(stored.SceneSystem, parsed.SceneSystem),
                TweakTextParser.MergeLists(stored.Video, parsed.Video)),
        };

        var fovNote = string.Empty;
        if (AspectRatio.Normalize(parsed.Value) is { } value)
        {
            settings = settings with { FovValue = value };
            fovNote = $" FOV set to {FovLabel(value)}.";
        }

        Save(settings);
        var imported = new ExtraTweaks(parsed.ConVars, parsed.SceneSystem, parsed.Video);
        notifier.Notify($"Imported {Counts(imported)}.{fovNote} Applying…");
        Apply(interactive: true, notify: true);
    }

    public void ViewTweaks()
    {
        var tweaks = Settings.Tweaks;
        var lines = new List<string>
        {
            $"FOV: r_aspectratio {FovLabel(Settings.FovValue)}",
            $"Apply extra tweaks: {(Settings.ApplyTweaks ? "yes" : "no")}",
        };
        foreach (var (title, entries) in new[] { ("ConVars", tweaks.ConVars), ("SceneSystem", tweaks.SceneSystem), ("video.cfg", tweaks.Video) })
        {
            lines.Add(string.Empty);
            lines.Add($"[{title}]  ({entries.Count})");
            lines.AddRange(entries.Select(entry => $"    {entry.Key}  {entry.Value}"));
        }

        prompts.ShowText("Stored tweaks", string.Join(Environment.NewLine, lines));
    }

    public void ClearTweaks()
    {
        if (!prompts.Confirm(
            "Clear all stored extra tweaks?\n\n" +
            "This only clears them from DL-FOV-Fixer. It does not edit or remove anything already in your gameinfo.gi."))
        {
            return;
        }

        Save(Settings with { Tweaks = ExtraTweaks.None });
        notifier.Notify("Cleared stored tweaks.");
    }

    public void ToggleApplyTweaks() => Save(Settings with { ApplyTweaks = !Settings.ApplyTweaks });

    public void SetTheme(ThemeMode theme)
    {
        if (theme != Settings.Theme)
        {
            Save(Settings with { Theme = theme });
        }
    }

    public void ToggleCheckUpdatesOnStart() => Save(Settings with { CheckUpdatesOnStart = !Settings.CheckUpdatesOnStart });

    public void ToggleAutoApply() => Save(Settings with { AutoApplyOnStart = !Settings.AutoApplyOnStart });

    public void ToggleSignInStartup()
    {
        var enable = !signInStartup.IsEnabled;
        signInStartup.SetEnabled(enable);
        Save(Settings with { StartWithWindows = enable });
    }

    public void OpenGameInfo()
    {
        if (!EnsureLocated(interactive: true))
        {
            notifier.Notify(NotLocated);
            return;
        }

        opener.Open(Settings.GameInfoPath);
    }

    public void LocateGameInfo()
    {
        if (prompts.AskGameInfoPath() is not { Length: > 0 } picked)
        {
            return;
        }

        if (!locator.LooksLikeGameInfo(picked))
        {
            prompts.Inform(NotGameInfo);
            return;
        }

        _applyProblem = null;
        Save(Settings with { GameInfoPath = Path.GetFullPath(picked) });
        notifier.Notify("gameinfo.gi location saved.");
    }

    public void ShowAbout()
    {
        var path = Settings.GameInfoPath;
        prompts.Inform(
            "DL-FOV-Fixer\n\n" +
            "Keeps Deadlock's FOV fix (r_aspectratio in gameinfo.gi) applied. It applies it again, with any " +
            "pasted extra config, after a game update wipes the file.\n\n" +
            $"File: {(path.Length > 0 ? path : "(not located)")}\n" +
            $"Target: r_aspectratio {FovLabel(Settings.FovValue)}\n" +
            $"Extra tweaks: {Counts(Settings.Tweaks)} ({(Settings.ApplyTweaks ? "on" : "off")})\n" +
            $"Update checks on start: {(Settings.CheckUpdatesOnStart ? "on" : "off")}\n" +
            $"Backup:{(HasGameInfo() ? files.BackupPathOf(path) : "(n/a)")}");
    }

    /// <summary>A value with its field of view when it is a number, like "2.49 (~101°)".</summary>
    public static string FovLabel(string value) =>
        AspectRatio.ToFovDegrees(value) is { } degrees ? $"{value} (~{degrees}°)" : value;

    public static string Describe(FixState state) => state switch
    {
        FixState.Ok => "up to date",
        FixState.NotApplied => "not applied yet",
        FixState.Drifted => "needs re-apply",
        FixState.Waiting => "waiting for Deadlock to close",
        FixState.Missing => "gameinfo.gi not located",
        _ => "last update failed",
    };

    public static string Counts(ExtraTweaks tweaks) =>
        $"{tweaks.ConVars.Count} convars, {tweaks.SceneSystem.Count} scenesystem, {tweaks.Video.Count} video";

    private void SetUpFirstRun()
    {
        if (EnsureLocated(interactive: true))
        {
            var current = Read(Settings.GameInfoPath) is { } text ? GameInfoMerge.ReadValue(text) : null;
            if (AspectRatio.Normalize(current) is { } value)
            {
                // Keep the value already in the file rather than replace it with the default.
                Settings = Settings with { FovValue = value };
            }
        }

        Save(Settings);
        notifier.Notify("DL-FOV-Fixer is running in the tray. It keeps your Deadlock FOV applied after updates.");
    }

    private void Apply(bool interactive, bool notify)
    {
        if (!EnsureLocated(interactive))
        {
            if (interactive)
            {
                notifier.Notify(NotLocated);
            }

            Refresh();
            return;
        }

        var outcome = RunApply();
        switch (outcome.Result)
        {
            case ApplyResult.Waiting when interactive:
                notifier.Notify("The file is in use. Is Deadlock running? It applies by itself once the game is closed.");
                break;
            case ApplyResult.Failed:
                notifier.Notify($"Failed to update files: {outcome.Error}");
                break;
            case ApplyResult.Applied or ApplyResult.UpToDate when notify:
                notifier.Notify(ApplyMessage(outcome));
                break;
        }
    }

    private ApplyOutcome RunApply()
    {
        var outcome = applier.Apply(Settings);
        var status = probe.After(outcome, Settings);
        _applyProblem = status.State is FixState.Waiting or FixState.Failed ? status : null;
        SetStatus(status);
        return outcome;
    }

    private string ApplyMessage(ApplyOutcome outcome)
    {
        var lead = outcome.Result == ApplyResult.Applied ? "Applied" : "Already up to date";
        var parts = new List<string> { $"FOV {FovLabel(Settings.FovValue)}" };
        if (Settings.ApplyTweaks && outcome.GameInfo is { } gameInfo)
        {
            var tweaks = Settings.Tweaks;
            var bits = new List<string>();

            // The first ConVars entry is always the FOV itself, so it does not count as a tweak.
            var conVars = Math.Max(Written(gameInfo.ConVars) - 1, 0);
            if (conVars > 0 || tweaks.ConVars.Count > 0)
            {
                bits.Add($"{conVars} convars");
            }

            if (tweaks.SceneSystem.Count > 0)
            {
                bits.Add($"{Written(gameInfo.SceneSystem)} scene");
            }

            if (tweaks.Video.Count > 0)
            {
                var video = outcome.Video;
                bits.Add($"{Written(video?.Keys ?? [])} video{(video?.Created == true ? " (created)" : string.Empty)}");
            }

            if (bits.Count > 0)
            {
                parts.Add(string.Join(" + ", bits));
            }

            var skipped = Skipped(gameInfo.ConVars) + Skipped(gameInfo.SceneSystem);
            if (skipped > 0)
            {
                parts.Add($"{skipped} skipped (nested)");
            }
        }

        return $"{lead}: {string.Join(" · ", parts)}.";
    }

    private static int Written(IEnumerable<KeyMerge> keys) =>
        keys.Count(key => key.Action is MergeAction.Updated or MergeAction.Added);

    private static int Skipped(IEnumerable<KeyMerge> keys) =>
        keys.Count(key => key.Action == MergeAction.SkippedBlock);

    /// <summary>Makes sure a gameinfo.gi path is stored. It finds one through Steam or, when allowed, asks.</summary>
    private bool EnsureLocated(bool interactive)
    {
        if (HasGameInfo())
        {
            return true;
        }

        if (locator.Locate() is { } found)
        {
            Save(Settings with { GameInfoPath = found });
            return true;
        }

        if (!interactive)
        {
            return false;
        }

        if (prompts.AskGameInfoPath() is not { Length: > 0 } picked)
        {
            return false;
        }

        if (!locator.LooksLikeGameInfo(picked))
        {
            prompts.Inform(NotGameInfo);
            return false;
        }

        Save(Settings with { GameInfoPath = Path.GetFullPath(picked) });
        return true;
    }

    private bool HasGameInfo()
    {
        var path = Settings.GameInfoPath;
        if (path.Length == 0)
        {
            return false;
        }

        try
        {
            return files.ReadText(path) is not null;
        }
        catch (GameFileException exception)
        {
            // A file the game has open is still there.
            return exception.IsLocked;
        }
    }

    private string? Read(string path)
    {
        try
        {
            return files.ReadText(path);
        }
        catch (GameFileException)
        {
            return null;
        }
    }

    private void Save(AppSettings settings)
    {
        Settings = settings;
        store.Save(settings);
        Refresh();
    }

    private void Refresh() => SetStatus(_applyProblem ?? probe.Probe(Settings));

    private void SetStatus(FixStatus status)
    {
        Status = status;
        Changed?.Invoke(this, EventArgs.Empty);
    }
}
