namespace DlFovFixer.Core.Locating;

/// <summary>Finds Deadlock's gameinfo.gi without asking the user.</summary>
public interface IGameInfoLocator
{
    /// <summary>The path of gameinfo.gi in the first Steam library that has Deadlock, or null.</summary>
    string? Locate();

    /// <summary>A cheap check that a file the user picked is a Deadlock gameinfo.gi.</summary>
    bool LooksLikeGameInfo(string path);
}
