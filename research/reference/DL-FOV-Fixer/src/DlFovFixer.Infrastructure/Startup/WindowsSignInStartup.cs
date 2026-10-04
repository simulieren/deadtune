using DlFovFixer.Core.Startup;

namespace DlFovFixer.Infrastructure.Startup;

/// <summary>
/// Starts the app at sign-in through a Run value named like 1.0's, so turning the setting off also
/// removes a value 1.0 left behind.
/// </summary>
public sealed class WindowsSignInStartup(IRunValues runValues, string executablePath) : ISignInStartup
{
    public const string ValueName = "DL-FOV-Fixer";

    public bool IsEnabled => runValues.Read(ValueName) is not null;

    private string Command => $"\"{executablePath}\"";

    public void SetEnabled(bool enabled)
    {
        if (enabled)
        {
            runValues.Write(ValueName, Command);
        }
        else
        {
            runValues.Delete(ValueName);
        }
    }

    public void Repair()
    {
        var current = runValues.Read(ValueName);
        if (current is not null && current != Command)
        {
            runValues.Write(ValueName, Command);
        }
    }
}
