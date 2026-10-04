using Microsoft.Win32;

namespace DlFovFixer.Infrastructure.Locating;

/// <summary>Reads one string value from the registry, so the locator can be tested without it.</summary>
public interface IRegistryReader
{
    /// <summary>The value's text, or null when the key or value is missing or not readable.</summary>
    string? ReadString(RegistryHive hive, string keyPath, string valueName);
}
