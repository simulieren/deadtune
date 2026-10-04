using DlFovFixer.Core.Applying;

namespace DlFovFixer.App.Tests.Support;

/// <summary>Files in memory. <see cref="WriteFailure"/>, when set, is thrown by every write.</summary>
internal sealed class FakeGameFiles : IGameFiles
{
    public Dictionary<string, string> Texts { get; } = new(StringComparer.OrdinalIgnoreCase);

    public List<string> Writes { get; } = [];

    public GameFileException? WriteFailure { get; set; }

    public string? ReadText(string path) => Texts.GetValueOrDefault(path);

    public void WriteText(string path, string text)
    {
        if (WriteFailure is not null)
        {
            throw WriteFailure;
        }

        Writes.Add(path);
        Texts[path] = text;
    }

    public string BackupPathOf(string path) => path + ".bak";

    public bool RestoreBackup(string path) => false;
}
