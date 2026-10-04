using DlFovFixer.Core.GameInfo;

namespace DlFovFixer.Core.Applying;

/// <summary>
/// What an apply did. <see cref="GameInfo"/> and <see cref="Video"/> are set once that file was
/// merged, and <see cref="Error"/> explains a <see cref="ApplyResult.Failed"/> or
/// <see cref="ApplyResult.Waiting"/> result.
/// </summary>
public sealed record ApplyOutcome(
    ApplyResult Result,
    GameInfoMergeOutcome? GameInfo = null,
    VideoConfigOutcome? Video = null,
    string? Error = null);
