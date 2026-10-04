using DlFovFixer.Infrastructure.Startup;

namespace DlFovFixer.Infrastructure.Tests.Startup;

public sealed class WindowsSignInStartupTests
{
    private const string Executable = @"D:\Apps\DL-FOV-Fixer\DL-FOV-Fixer.exe";

    private readonly FakeRunValues _runValues = new();

    [Fact]
    public void SetEnabled_True_WritesTheQuotedExecutableUnderThe10Name()
    {
        new WindowsSignInStartup(_runValues, Executable).SetEnabled(true);

        Assert.Equal($"\"{Executable}\"", _runValues["DL-FOV-Fixer"]);
    }

    [Fact]
    public void SetEnabled_False_RemovesAValue10LeftBehind()
    {
        _runValues["DL-FOV-Fixer"] = "\"D:\\Tools\\DL-FOV-Fixer.exe\"";
        var startup = new WindowsSignInStartup(_runValues, Executable);

        startup.SetEnabled(false);

        Assert.False(startup.IsEnabled);
        Assert.Empty(_runValues);
    }

    [Fact]
    public void IsEnabled_FollowsTheRunValue()
    {
        var startup = new WindowsSignInStartup(_runValues, Executable);
        Assert.False(startup.IsEnabled);

        _runValues["DL-FOV-Fixer"] = "anything";

        Assert.True(startup.IsEnabled);
    }

    [Fact]
    public void Repair_AValue10LeftBehind_PointsItAtThisExecutable()
    {
        _runValues["DL-FOV-Fixer"] = "\"D:\\Tools\\DL-FOV-Fixer.exe\"";

        new WindowsSignInStartup(_runValues, Executable).Repair();

        Assert.Equal($"\"{Executable}\"", _runValues["DL-FOV-Fixer"]);
    }

    [Fact]
    public void Repair_StartupOff_LeavesItOff()
    {
        new WindowsSignInStartup(_runValues, Executable).Repair();

        Assert.Empty(_runValues);
    }

    private sealed class FakeRunValues : Dictionary<string, string>, IRunValues
    {
        public string? Read(string name) => TryGetValue(name, out var command) ? command : null;

        public void Write(string name, string command) => this[name] = command;

        public void Delete(string name) => Remove(name);
    }
}
