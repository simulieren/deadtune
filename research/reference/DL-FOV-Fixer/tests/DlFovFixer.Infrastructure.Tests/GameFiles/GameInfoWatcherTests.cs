using DlFovFixer.Infrastructure.GameFiles;
using DlFovFixer.Infrastructure.Tests.Support;

namespace DlFovFixer.Infrastructure.Tests.GameFiles;

/// <summary>Real files in a temp folder. Waits are generous so a slow CI machine does not flake.</summary>
public sealed class GameInfoWatcherTests : IDisposable
{
    private static readonly TimeSpan Quiet = TimeSpan.FromMilliseconds(200);
    private static readonly TimeSpan Patience = TimeSpan.FromSeconds(5);

    private readonly TempFolder _folder = new();
    private readonly GameInfoWatcher _watcher = new(Quiet);
    private int _changes;

    public GameInfoWatcherTests() => _watcher.Changed += (_, _) => Interlocked.Increment(ref _changes);

    public void Dispose()
    {
        _watcher.Dispose();
        _folder.Dispose();
    }

    [Fact]
    public void Watch_FileRewritten_FiresOnce()
    {
        var path = _folder.WriteBytes("gameinfo.gi", [1]);
        _watcher.Watch(path);

        // A burst like a game update: several writes close together.
        for (var i = 0; i < 5; i++)
        {
            File.WriteAllBytes(path, [(byte)i, 2, 3]);
            Thread.Sleep(20);
        }

        Assert.True(WaitFor(() => _changes >= 1));
        Thread.Sleep(Quiet * 3);
        Assert.Equal(1, _changes);
    }

    [Fact]
    public void Watch_FileReplacedThroughATempFile_Fires()
    {
        var path = _folder.WriteBytes("gameinfo.gi", [1]);
        _watcher.Watch(path);

        var temp = _folder.WriteBytes("gameinfo.gi.tmp", [2]);
        File.Replace(temp, path, null);

        Assert.True(WaitFor(() => _changes >= 1));
    }

    [Fact]
    public void Watch_AnotherFileInTheFolder_StaysQuiet()
    {
        var path = _folder.WriteBytes("gameinfo.gi", [1]);
        _watcher.Watch(path);

        _folder.WriteBytes("gameinfo.gi.dlfovfixer.bak", [1]);
        _folder.WriteBytes("gameinfo_branchspecific.gi", [1]);

        Thread.Sleep(Quiet * 4);
        Assert.Equal(0, _changes);
    }

    [Fact]
    public void Watch_AnotherPath_StopsWatchingTheFirst()
    {
        var first = _folder.WriteBytes(Path.Combine("a", "gameinfo.gi"), [1]);
        var second = _folder.WriteBytes(Path.Combine("b", "gameinfo.gi"), [1]);
        _watcher.Watch(first);
        _watcher.Watch(second);

        File.WriteAllBytes(first, [9]);
        Thread.Sleep(Quiet * 4);
        Assert.Equal(0, _changes);

        File.WriteAllBytes(second, [9]);
        Assert.True(WaitFor(() => _changes >= 1));
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData(@"Z:\no\such\folder\gameinfo.gi")]
    public void Watch_NothingToWatch_DoesNotThrow(string? path) => _watcher.Watch(path);

    private static bool WaitFor(Func<bool> condition)
    {
        var deadline = DateTime.UtcNow + Patience;
        while (DateTime.UtcNow < deadline)
        {
            if (condition())
            {
                return true;
            }

            Thread.Sleep(25);
        }

        return condition();
    }
}
