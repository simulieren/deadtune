namespace DlFovFixer.Core.Applying;

/// <summary>
/// The state of the fix, the value found in the file when it was read, and the error behind a
/// <see cref="FixState.Waiting"/> or <see cref="FixState.Failed"/> state.
/// </summary>
public sealed record FixStatus(FixState State, string? CurrentValue = null, string? Error = null);
