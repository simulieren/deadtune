using DlFovFixer.Core.Applying;

namespace DlFovFixer.Core.Tests.Applying;

/// <summary>
/// Files in memory. <see cref="Failure"/>, when set, is thrown by every write, and
/// <see cref="ReadFailure"/> by every read.
/// </summary>
internal sealed class FakeGameFiles : IGameFiles
{
    public Dictionary<string, string> Texts { get; } = new(StringComparer.OrdinalIgnoreCase);

    public List<string> Writes { get; } = [];

    public GameFileException? Failure { get; set; }

    public GameFileException? ReadFailure { get; set; }

    public string? ReadText(string path) => ReadFailure is not null ? throw ReadFailure : Texts.GetValueOrDefault(path);

    public void WriteText(string path, string text)
    {
        if (Failure is not null)
        {
            throw Failure;
        }

        Writes.Add(path);
        Texts[path] = text;
    }

    public string BackupPathOf(string path) => path + ".bak";

    public bool RestoreBackup(string path) => false;
}
