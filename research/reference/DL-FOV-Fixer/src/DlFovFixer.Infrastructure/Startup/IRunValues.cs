namespace DlFovFixer.Infrastructure.Startup;

/// <summary>The current user's Run registry values, so sign-in startup can be tested without the registry.</summary>
public interface IRunValues
{
    string? Read(string name);

    void Write(string name, string command);

    void Delete(string name);
}
