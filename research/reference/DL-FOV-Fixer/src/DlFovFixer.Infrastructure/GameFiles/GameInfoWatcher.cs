namespace DlFovFixer.Infrastructure.GameFiles;

/// <summary>
/// Tells when gameinfo.gi changed on disk, so a game update that resets it is caught at once rather
/// than at the next periodic check. A game update writes many files in a burst, so
/// <see cref="Changed"/> fires once the file has been quiet for the quiet period. It fires on a
/// thread pool thread. The app's own writes fire it too, and the apply that follows finds nothing
/// to change.
/// </summary>
public sealed class GameInfoWatcher : IDisposable
{
    private readonly Lock _gate = new();
    private readonly TimeSpan _quietPeriod;
    private readonly Timer _quiet;
    private FileSystemWatcher? _watcher;
    private string? _path;
    private bool _disposed;

    public GameInfoWatcher(TimeSpan quietPeriod)
    {
        _quietPeriod = quietPeriod;
        _quiet = new Timer(_ => Fire(), null, Timeout.Infinite, Timeout.Infinite);
    }

    public event EventHandler? Changed;

    /// <summary>
    /// Watches <paramref name="path"/> instead of the file watched so far. An empty path, or one
    /// whose folder does not exist, stops watching.
    /// </summary>
    public void Watch(string? path)
    {
        lock (_gate)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            if (_watcher is not null && string.Equals(path, _path, StringComparison.OrdinalIgnoreCase))
            {
                return;
            }

            StopWatching();
            var folder = string.IsNullOrEmpty(path) ? null : Path.GetDirectoryName(path);
            if (folder is null || !Directory.Exists(folder))
            {
                return;
            }

            var watcher = new FileSystemWatcher(folder, Path.GetFileName(path)!)
            {
                NotifyFilter = NotifyFilters.LastWrite | NotifyFilters.Size | NotifyFilters.FileName | NotifyFilters.CreationTime,
            };
            watcher.Changed += (_, _) => Poke();
            watcher.Created += (_, _) => Poke();
            watcher.Renamed += (_, _) => Poke();

            // An overflowed buffer means changes were missed, so it counts as a change.
            watcher.Error += (_, _) => Poke();
            watcher.EnableRaisingEvents = true;
            _watcher = watcher;
            _path = path;
        }
    }

    public void Dispose()
    {
        lock (_gate)
        {
            if (_disposed)
            {
                return;
            }

            _disposed = true;
            StopWatching();
            _quiet.Dispose();
        }
    }

    // Every event pushes the deadline back, so a burst of writes ends in one Changed.
    private void Poke()
    {
        lock (_gate)
        {
            if (!_disposed && _watcher is not null)
            {
                _quiet.Change(_quietPeriod, Timeout.InfiniteTimeSpan);
            }
        }
    }

    private void Fire()
    {
        lock (_gate)
        {
            if (_disposed || _watcher is null)
            {
                return;
            }
        }

        Changed?.Invoke(this, EventArgs.Empty);
    }

    private void StopWatching()
    {
        _quiet.Change(Timeout.Infinite, Timeout.Infinite);
        _watcher?.Dispose();
        _watcher = null;
        _path = null;
    }
}
