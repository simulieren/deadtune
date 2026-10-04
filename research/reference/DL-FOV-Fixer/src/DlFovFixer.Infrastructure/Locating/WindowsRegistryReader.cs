using System.Security;
using Microsoft.Win32;

namespace DlFovFixer.Infrastructure.Locating;

/// <summary>The real registry, in its default view for this process.</summary>
public sealed class WindowsRegistryReader : IRegistryReader
{
    public string? ReadString(RegistryHive hive, string keyPath, string valueName)
    {
        try
        {
            using var root = RegistryKey.OpenBaseKey(hive, RegistryView.Default);
            using var key = root.OpenSubKey(keyPath);
            return key?.GetValue(valueName) as string;
        }
        catch (Exception exception) when (exception is IOException or SecurityException or UnauthorizedAccessException)
        {
            return null;
        }
    }
}
