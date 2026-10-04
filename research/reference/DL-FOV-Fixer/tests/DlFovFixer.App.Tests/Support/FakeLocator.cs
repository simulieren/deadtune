using System.IO;
using DlFovFixer.Core.Locating;

namespace DlFovFixer.App.Tests.Support;

/// <summary>Finds <see cref="Found"/>, and takes any picked file named gameinfo.gi.</summary>
internal sealed class FakeLocator : IGameInfoLocator
{
    public string? Found { get; set; }

    public string? Locate() => Found;

    public bool LooksLikeGameInfo(string path) => Path.GetFileName(path) == "gameinfo.gi";
}
