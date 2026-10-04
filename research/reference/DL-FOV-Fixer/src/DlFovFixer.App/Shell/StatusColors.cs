using DlFovFixer.Core.Applying;

namespace DlFovFixer.App.Shell;

/// <summary>Which color each <see cref="FixState"/> shows.</summary>
public static class StatusColors
{
    public static StatusColor Of(FixState state) => state switch
    {
        FixState.Ok => StatusColor.Green,
        FixState.NotApplied or FixState.Drifted or FixState.Waiting => StatusColor.Amber,
        _ => StatusColor.Red,
    };
}
