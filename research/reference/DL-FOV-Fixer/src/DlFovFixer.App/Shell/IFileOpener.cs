namespace DlFovFixer.App.Shell;

/// <summary>Opens a file in the program Windows has for it.</summary>
public interface IFileOpener
{
    void Open(string path);
}
