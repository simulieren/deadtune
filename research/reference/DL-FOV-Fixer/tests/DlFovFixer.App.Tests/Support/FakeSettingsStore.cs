using DlFovFixer.Core.Settings;

namespace DlFovFixer.App.Tests.Support;

/// <summary>Settings in memory. <see cref="Saved"/> is null until the first save.</summary>
internal sealed class FakeSettingsStore(AppSettings? saved = null) : ISettingsStore
{
    public AppSettings? Saved { get; private set; } = saved;

    public bool Exists => Saved is not null;

    public AppSettings Load() => Saved ?? AppSettings.Defaults;

    public void Save(AppSettings settings) => Saved = settings;
}
