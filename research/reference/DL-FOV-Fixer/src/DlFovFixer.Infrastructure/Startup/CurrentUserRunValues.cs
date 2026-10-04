using Microsoft.Win32;

namespace DlFovFixer.Infrastructure.Startup;

/// <summary><c>HKCU\Software\Microsoft\Windows\CurrentVersion\Run</c>, which needs no elevation.</summary>
public sealed class CurrentUserRunValues : IRunValues
{
    private const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";

    public string? Read(string name)
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKey);
        return key?.GetValue(name) as string;
    }

    public void Write(string name, string command)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKey);
        key.SetValue(name, command, RegistryValueKind.String);
    }

    public void Delete(string name)
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKey, writable: true);
        key?.DeleteValue(name, throwOnMissingValue: false);
    }
}
