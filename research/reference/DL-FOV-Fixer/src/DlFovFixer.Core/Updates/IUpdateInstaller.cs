namespace DlFovFixer.Core.Updates;

/// <summary>Starts a checked installer. The app exits afterwards so the installer can replace it.</summary>
public interface IUpdateInstaller
{
    void Launch(string localPath);
}
