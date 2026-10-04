namespace DlFovFixer.Core.Applying;

/// <summary>
/// Reads and writes the game's text files. An implementation keeps every byte it is given, writes no
/// byte-order mark it was not given, and takes the one-time backup before the first change.
/// </summary>
public interface IGameFiles
{
    /// <summary>The file's text, or null when it does not exist.</summary>
    /// <exception cref="GameFileException">The file is locked or cannot be read.</exception>
    string? ReadText(string path);

    /// <summary>
    /// Replaces the file's text, creating it and its folder when missing. Before the first change to
    /// an existing file it is copied to its one-time backup, and an existing backup is never replaced.
    /// </summary>
    /// <exception cref="GameFileException">The file is locked or cannot be written.</exception>
    void WriteText(string path, string text);

    /// <summary>Where the one-time backup of <paramref name="path"/> is kept.</summary>
    string BackupPathOf(string path);

    /// <summary>Copies the one-time backup back over the file. False when there is no backup.</summary>
    /// <exception cref="GameFileException">The file is locked or cannot be written.</exception>
    bool RestoreBackup(string path);
}
