using DlFovFixer.Core.GameInfo;
using DlFovFixer.Core.Settings;

namespace DlFovFixer.Core.Applying;

/// <summary>Reads gameinfo.gi, without writing it, to tell where the fix stands.</summary>
public sealed class StatusProbe(IGameFiles files)
{
    public FixStatus Probe(AppSettings settings)
    {
        var path = settings.GameInfoPath;
        if (string.IsNullOrEmpty(path))
        {
            return new FixStatus(FixState.Missing);
        }

        string? text;
        try
        {
            text = files.ReadText(path);
        }
        catch (GameFileException exception)
        {
            return new FixStatus(exception.IsLocked ? FixState.Waiting : FixState.Failed, Error: exception.Message);
        }

        if (text is null)
        {
            return new FixStatus(FixState.Missing);
        }

        var current = GameInfoMerge.ReadValue(text);
        if (current is null)
        {
            return new FixStatus(FixState.NotApplied);
        }

        var target = AspectRatio.Normalize(settings.FovValue) ?? AspectRatio.DefaultValue;
        return new FixStatus(current == target ? FixState.Ok : FixState.Drifted, current);
    }

    /// <summary>
    /// The status after an apply. A lock or a failure is what the apply ran into, so it stands until
    /// the next apply. Otherwise the file is read again, since a written file is the only proof.
    /// </summary>
    public FixStatus After(ApplyOutcome outcome, AppSettings settings) => outcome.Result switch
    {
        ApplyResult.Waiting => new FixStatus(FixState.Waiting, Error: outcome.Error),
        ApplyResult.Failed => new FixStatus(FixState.Failed, Error: outcome.Error),
        ApplyResult.Missing => new FixStatus(FixState.Missing),
        _ => Probe(settings),
    };
}
