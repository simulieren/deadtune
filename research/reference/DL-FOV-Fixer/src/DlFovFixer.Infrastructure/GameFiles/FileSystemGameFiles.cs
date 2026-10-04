using System.Text;
using DlFovFixer.Core.Applying;

namespace DlFovFixer.Infrastructure.GameFiles;

/// <summary>
/// The game's files on disk. Text goes through UTF-8 without adding a byte-order mark, so a file
/// read and written back unchanged keeps every byte, its own line endings included.
/// </summary>
public sealed class FileSystemGameFiles : IGameFiles
{
    public const string BackupSuffix = ".dlfovfixer.bak";

    private const string TempSuffix = ".dlfovfixer.tmp";

    // The Win32 errors for a file another process has open or locked: the Waiting state.
    private const int ErrorSharingViolation = 32;
    private const int ErrorLockViolation = 33;

    private static readonly UTF8Encoding Utf8 = new(encoderShouldEmitUTF8Identifier: false);

    public string? ReadText(string path) => Guard(path, "read", () =>
    {
        try
        {
            return Utf8.GetString(File.ReadAllBytes(path));
        }
        catch (Exception exception) when (exception is FileNotFoundException or DirectoryNotFoundException)
        {
            return null;
        }
    });

    public void WriteText(string path, string text) => Guard(path, "write", () =>
    {
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        var exists = File.Exists(path);
        if (exists)
        {
            var backup = BackupPathOf(path);
            if (!File.Exists(backup))
            {
                File.Copy(path, backup, overwrite: false);
            }
        }

        var temp = path + TempSuffix;
        try
        {
            File.WriteAllBytes(temp, Utf8.GetBytes(text));
            if (exists)
            {
                // File.Replace, not File.Move with overwrite: onto a file the game holds open, Move
                // fails with "access denied" and Replace with the sharing violation that means
                // Waiting (docs/pitfalls.md).
                File.Replace(temp, path, destinationBackupFileName: null);
            }
            else
            {
                File.Move(temp, path);
            }
        }
        finally
        {
            File.Delete(temp);
        }

        return true;
    });

    public string BackupPathOf(string path) => path + BackupSuffix;

    public bool RestoreBackup(string path) => Guard(path, "restore", () =>
    {
        var backup = BackupPathOf(path);
        if (!File.Exists(backup))
        {
            return false;
        }

        File.Copy(backup, path, overwrite: true);
        return true;
    });

    private static T Guard<T>(string path, string verb, Func<T> action)
    {
        try
        {
            return action();
        }
        catch (IOException exception) when ((exception.HResult & 0xFFFF) is ErrorSharingViolation or ErrorLockViolation)
        {
            throw new GameFileException($"{Path.GetFileName(path)} is in use by another program, probably the game.", isLocked: true, exception);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            throw new GameFileException($"Could not {verb} {path}: {exception.Message}", isLocked: false, exception);
        }
    }
}
