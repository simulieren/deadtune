using System.Text;
using DlFovFixer.Core.Applying;
using DlFovFixer.Infrastructure.GameFiles;
using DlFovFixer.Infrastructure.Tests.Support;

namespace DlFovFixer.Infrastructure.Tests.GameFiles;

public sealed class FileSystemGameFilesTests : IDisposable
{
    private readonly TempFolder _folder = new();
    private readonly FileSystemGameFiles _files = new();

    public static TheoryData<string> Encodings => ["plain LF", "CRLF", "mixed endings", "byte-order mark", "no final newline", "non-ASCII"];

    public void Dispose() => _folder.Dispose();

    [Theory]
    [MemberData(nameof(Encodings))]
    public void ReadThenWriteUnchanged_KeepsEveryByte(string shape)
    {
        var bytes = Sample(shape);
        var path = _folder.WriteBytes("gameinfo.gi", bytes);

        _files.WriteText(path, _files.ReadText(path)!);

        Assert.Equal(bytes, File.ReadAllBytes(path));
    }

    [Fact]
    public void ReadText_MissingFile_IsNull()
    {
        Assert.Null(_files.ReadText(_folder.File("missing", "gameinfo.gi")));
    }

    [Fact]
    public void WriteText_ExistingFile_TakesTheOneTimeBackupFirst()
    {
        var path = _folder.WriteBytes("gameinfo.gi", "original"u8.ToArray());

        _files.WriteText(path, "first change");

        Assert.Equal("original", File.ReadAllText(_files.BackupPathOf(path)));
        Assert.Equal("first change", File.ReadAllText(path));
    }

    [Fact]
    public void WriteText_BackupExists_NeverReplacesIt()
    {
        var path = _folder.WriteBytes("gameinfo.gi", "original"u8.ToArray());
        _files.WriteText(path, "first change");

        _files.WriteText(path, "second change");

        Assert.Equal("original", File.ReadAllText(_files.BackupPathOf(path)));
        Assert.Equal("second change", File.ReadAllText(path));
    }

    [Fact]
    public void WriteText_NewFile_CreatesItsFolderAndNoBackup()
    {
        var path = _folder.File("cfg", "video.txt");

        _files.WriteText(path, "\"video.cfg\"\r\n{\r\n}\r\n");

        Assert.Equal("\"video.cfg\"\r\n{\r\n}\r\n"u8.ToArray(), File.ReadAllBytes(path));
        Assert.False(File.Exists(_files.BackupPathOf(path)));
    }

    [Fact]
    public void WriteText_LeavesNoTemporaryFileBehind()
    {
        var path = _folder.WriteBytes("gameinfo.gi", "original"u8.ToArray());

        _files.WriteText(path, "changed");

        Assert.Equal(["gameinfo.gi", "gameinfo.gi" + FileSystemGameFiles.BackupSuffix], Directory.GetFiles(_folder.Path).Select(Path.GetFileName).Order(StringComparer.Ordinal));
    }

    [Theory]
    [InlineData(FileShare.None)]
    [InlineData(FileShare.Read)]
    public void WriteText_FileOpenInTheGame_IsLockedAndLeavesTheFileAlone(FileShare gameSharing)
    {
        var path = _folder.WriteBytes("gameinfo.gi", "original"u8.ToArray());
        using (new FileStream(path, FileMode.Open, FileAccess.Read, gameSharing))
        {
            var exception = Assert.Throws<GameFileException>(() => _files.WriteText(path, "changed"));

            Assert.True(exception.IsLocked);
        }

        Assert.Equal("original", File.ReadAllText(path));
        Assert.DoesNotContain(Directory.GetFiles(_folder.Path), file => file.EndsWith(".tmp", StringComparison.Ordinal));
    }

    [Fact]
    public void ReadText_FileOpenWithoutSharing_IsLocked()
    {
        var path = _folder.WriteBytes("gameinfo.gi", "original"u8.ToArray());
        using var game = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None);

        var exception = Assert.Throws<GameFileException>(() => _files.ReadText(path));

        Assert.True(exception.IsLocked);
    }

    [Fact]
    public void WriteText_ReadOnlyFolderOrFile_FailsWithoutLocked()
    {
        var path = _folder.WriteBytes("gameinfo.gi", "original"u8.ToArray());
        File.SetAttributes(path, FileAttributes.ReadOnly);
        try
        {
            var exception = Assert.Throws<GameFileException>(() => _files.WriteText(path, "changed"));

            Assert.False(exception.IsLocked);
        }
        finally
        {
            File.SetAttributes(path, FileAttributes.Normal);
        }
    }

    [Fact]
    public void RestoreBackup_PutsTheOriginalBack()
    {
        var path = _folder.WriteBytes("gameinfo.gi", "original"u8.ToArray());
        _files.WriteText(path, "changed");

        Assert.True(_files.RestoreBackup(path));

        Assert.Equal("original", File.ReadAllText(path));
    }

    [Fact]
    public void RestoreBackup_NoBackup_ReturnsFalse()
    {
        var path = _folder.WriteBytes("gameinfo.gi", "original"u8.ToArray());

        Assert.False(_files.RestoreBackup(path));
        Assert.Equal("original", File.ReadAllText(path));
    }

    private static byte[] Sample(string shape) => shape switch
    {
        "plain LF" => "\"GameInfo\"\n{\n\tConVars\n\t{\n\t}\n}\n"u8.ToArray(),
        "CRLF" => "\"GameInfo\"\r\n{\r\n\tConVars\r\n\t{\r\n\t}\r\n}\r\n"u8.ToArray(),
        "mixed endings" => "\"GameInfo\"\r\n{\n\tConVars\r\n\t{\n\t}\r\n}\n"u8.ToArray(),
        "byte-order mark" => [0xEF, 0xBB, 0xBF, .. "\"GameInfo\"\r\n{\r\n}\r\n"u8],
        "no final newline" => "\"GameInfo\"\n{\n}"u8.ToArray(),
        "non-ASCII" => Encoding.UTF8.GetBytes("\"GameInfo\"\n{\n\t// Lukáš's ∞ tweaks\n}\n"),
        _ => throw new ArgumentOutOfRangeException(nameof(shape)),
    };
}
