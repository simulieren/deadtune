namespace DlFovFixer.App.Tests.Support;

/// <summary>
/// Tests that create a WPF Application. While one exists, every WPF element made on another thread
/// takes its implicit styles, whose brushes belong to the Application's thread, and throws "Cannot
/// access Freezable ... across threads". Parallel runs are off for this collection, so xUnit runs it
/// on its own after the other tests.
/// </summary>
[CollectionDefinition(Name, DisableParallelization = true)]
public sealed class WpfApplicationCollection
{
    public const string Name = "WPF Application";
}
