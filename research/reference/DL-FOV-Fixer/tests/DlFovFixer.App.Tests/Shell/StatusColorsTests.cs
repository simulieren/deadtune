using DlFovFixer.App.Shell;
using DlFovFixer.Core.Applying;

namespace DlFovFixer.App.Tests.Shell;

/// <summary>Six states behind three colors (docs/csharp-rewrite.md, Status model).</summary>
public sealed class StatusColorsTests
{
    [Theory]
    [InlineData(FixState.Ok, StatusColor.Green)]
    [InlineData(FixState.NotApplied, StatusColor.Amber)]
    [InlineData(FixState.Drifted, StatusColor.Amber)]
    [InlineData(FixState.Waiting, StatusColor.Amber)]
    [InlineData(FixState.Missing, StatusColor.Red)]
    [InlineData(FixState.Failed, StatusColor.Red)]
    public void Of_MapsEveryState(FixState state, StatusColor expected) =>
        Assert.Equal(expected, StatusColors.Of(state));

    [Fact]
    public void Of_CoversEveryState() =>
        Assert.All(Enum.GetValues<FixState>(), state => Assert.True(Enum.IsDefined(StatusColors.Of(state))));
}
