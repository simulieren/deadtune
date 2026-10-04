using System.Windows;
using DlFovFixer.App.Composition;
using DlFovFixer.App.Startup;
using DotNetLib.Tray;

namespace DlFovFixer.App;

/// <summary>
/// The WPF entry point. It keeps one instance per user, builds the graph and runs until Quit.
/// </summary>
public partial class App : Application
{
    private const string InstanceName = "DL-FOV-Fixer";

    private SingleInstance? _instance;
    private AppGraph? _graph;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        _instance = SingleInstance.TryAcquire(InstanceName);
        if (_instance is null)
        {
            SingleInstance.Knock(InstanceName);
            Shutdown();
            return;
        }

        TrayResources.Merge(Resources);
        _graph = new AppGraph(BuildInfo.Current, StartupOptions.Parse(e.Args), Shutdown);
        _instance.Listen(() => Dispatcher.BeginInvoke(() => _graph?.OnLaunchedAgain()));
        _graph.Start();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _graph?.Dispose();
        _instance?.Dispose();
        base.OnExit(e);
    }
}
