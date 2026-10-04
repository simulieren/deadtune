namespace DlFovFixer.Core.Applying;

/// <summary>
/// Reading or writing a game file failed. <see cref="IsLocked"/> means another process, usually the
/// running game, has the file open: that is the Waiting state, not an error.
/// </summary>
public sealed class GameFileException(string message, bool isLocked, Exception? innerException = null)
    : Exception(message, innerException)
{
    public bool IsLocked { get; } = isLocked;
}
