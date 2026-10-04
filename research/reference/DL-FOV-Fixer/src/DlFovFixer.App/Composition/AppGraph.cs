using System.IO;
using System.Net.Http;
using System.Windows.Threading;
using DlFovFixer.App.Shell;
using DlFovFixer.App.Startup;
using DlFovFixer.App.Theming;
using DlFovFixer.App.ViewModels;
using DlFovFixer.Core.Applying;
using DlFovFixer.Core.Settings;
using DlFovFixer.Core.Updates;
using DlFovFixer.Infrastructure.GameFiles;
using DlFovFixer.Infrastructure.Locating;
using DlFovFixer.Infrastructure.Settings;
using DlFovFixer.Infrastructure.Startup;
using DlFovFixer.Infrastructure.Updates;
using DotNetLib.Tray;

namespace DlFovFixer.App.Composition;

/// <summary>
/// The only composition root. It builds the adapters, the use cases and the tray. A watcher on
/// gameinfo.gi re-applies the fix as soon as a game update resets it, and the periodic timer is the
/// fallback for a change the watcher misses.
/// </summary>
public sealed class AppGraph : IDisposable
{
    /// <summary>The repository whose releases are the update channel.</summary>
    public const string RepositoryUrl = "https://github.com/lukr-99/DL-FOV-Fixer";

    private readonly TrayIconHost _tray;
    private readonly TrayNotifier _notifier;
    private readonly TrayViewModel _model;
    private readonly UpdatesViewModel _updates;
    private readonly HttpClient _http;
    private readonly EcdsaSignatureVerifier? _signatures;
    private readonly TrayThemeApplier _theme;
    private readonly DispatcherTimer _timer = new();
    private readonly GameInfoWatcher _watcher = new(WatcherQuietPeriod);

    // Long enough for a game update to finish writing the file.
    private static readonly TimeSpan WatcherQuietPeriod = TimeSpan.FromSeconds(3);

    public AppGraph(BuildInfo build, StartupOptions options, Action quit)
    {
        var title = build.IsRelease ? TrayViewModel.AppTitle : $"{TrayViewModel.AppTitle} (dev)";
        var files = new FileSystemGameFiles();
        var signInStartup = new WindowsSignInStartup(new CurrentUserRunValues(), Environment.ProcessPath!);
        if (build.IsRelease && options.SettingsPath is null)
        {
            // A dev build must never take over the Run value an installed copy relies on.
            signInStartup.Repair();
        }

        _theme = new TrayThemeApplier(
            System.Windows.Application.Current.Resources,
            TrayThemeApplier.WindowsAppsUseDark,
            WarmPalettes.Light,
            WarmPalettes.Dark);
        var prompts = new WpfUserPrompts(TrayViewModel.AppTitle, _theme.Attach);
        var opener = new ShellFileOpener();
        TrayViewModel? model = null;
        _tray = new TrayIconHost(title, () => model?.ApplyNow());
        _tray.SetIcon(StatusIconFactory.CreateIcon(StatusColor.Amber));
        _notifier = new TrayNotifier(_tray, title);
        _model = model = new TrayViewModel(
            new JsonSettingsStore(options.SettingsPath ?? JsonSettingsStore.DefaultPath),
            new ApplyService(files),
            new StatusProbe(files),
            new SteamGameInfoLocator(new WindowsRegistryReader(), SteamGameInfoLocator.CommonSteamFolders),
            signInStartup,
            files,
            prompts,
            _notifier,
            opener);

        _http = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
        _http.DefaultRequestHeaders.UserAgent.ParseAdd($"DL-FOV-Fixer/{build.Version}");
        var address = new ReleaseChannelAddress(RepositoryUrl);
        var downloads = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "DL-FOV-Fixer", "Updates");
        var hasKey = build.ManifestPublicKey.Length > 0;
        _signatures = hasKey ? new EcdsaSignatureVerifier(build.ManifestPublicKey) : null;
        var service = new UpdateService(
            build.Version,
            channelConfigured: hasKey,
            new GitHubReleaseChannel(_http, address, downloads),
            _signatures ?? (ISignatureVerifier)new NoSignatureVerifier(),
            new InstallerLauncher());
        _updates = new UpdatesViewModel(service, build.Version, address.ReleasesPage, prompts, _notifier, opener);

        void Render()
        {
            _tray.SetIcon(StatusIconFactory.CreateIcon(StatusColors.Of(_model.Status.State)));
            _tray.SetToolTip($"{title}: {TrayViewModel.Describe(_model.Status.State)}");
            _tray.SetMenu(TrayMenu.Build(_model, _updates, quit));
        }

        var dispatcher = Dispatcher.CurrentDispatcher;
        _model.Changed += (_, _) =>
        {
            var mode = ToTrayMode(_model.Settings.Theme);
            if (mode != _theme.Mode)
            {
                _theme.Apply(mode);
            }

            Render();
            _watcher.Watch(_model.Settings.GameInfoPath);
        };
        _theme.Applied += (_, _) => Render();
        _theme.Apply(ToTrayMode(_model.Settings.Theme));
        _watcher.Changed += (_, _) => dispatcher.BeginInvoke(_model.Tick);
        _updates.Changed += (_, _) => Render();
        _updates.ExitRequested += (_, _) => quit();
        _timer.Tick += (_, _) => _model.Tick();
    }

    /// <summary>Called when the app is launched again while this one runs.</summary>
    public void OnLaunchedAgain() =>
        _notifier.Notify("DL-FOV-Fixer is already running. Right-click its icon in the tray for the menu.");

    public void Start()
    {
        _model.Start();
        var minutes = _model.Settings.PeriodicCheckMinutes;
        if (minutes > 0)
        {
            _timer.Interval = TimeSpan.FromMinutes(minutes);
            _timer.Start();
        }

        if (_model.Settings.CheckUpdatesOnStart)
        {
            _ = _updates.CheckAsync(interactive: false);
        }
    }

    /// <summary>The saved theme setting as the tray kit's mode.</summary>
    internal static TrayThemeMode ToTrayMode(ThemeMode mode) => mode switch
    {
        ThemeMode.Light => TrayThemeMode.Light,
        ThemeMode.Dark => TrayThemeMode.Dark,
        _ => TrayThemeMode.System,
    };

    public void Dispose()
    {
        _timer.Stop();
        _watcher.Dispose();
        _theme.Dispose();
        _tray.Dispose();
        _http.Dispose();
        _signatures?.Dispose();
    }
}
