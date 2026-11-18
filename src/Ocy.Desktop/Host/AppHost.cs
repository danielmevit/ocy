using Avalonia.Threading;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Persistence;
using Ocy.Core.Platform;
using Ocy.Core.Settings;
using Ocy.Desktop.Coordinating;
using Ocy.Desktop.Localization;
using Ocy.Desktop.Tray;
using Ocy.Desktop.Windows;
using Ocy.Platform.Windows;

namespace Ocy.Desktop.Host;

public sealed class AppHost : IDisposable
{
    private readonly DispatcherTimer _timer;
    private readonly Action? _shutdownAction;
    private readonly TimeProvider _timeProvider;
    private int _secondsSinceSave;
    private bool _disposed;

    public string DataDirectory { get; }

    public AppDataStore Store { get; }

    public IPlatformServices PlatformServices { get; }

    public BreakEngine Engine { get; }

    public Strings Strings { get; }

    public TrayPresenter TrayPresenter { get; }

    public IBreakWindows BreakWindows { get; }

    public ISettingsWindow SettingsWindow { get; }

    public BreakUiCoordinator Coordinator { get; }

    public TrayController? TrayController { get; private set; }

    public AppHost(
        Action? shutdownAction = null,
        IBreakWindows? breakWindows = null,
        ISettingsWindow? settingsWindow = null,
        TimeProvider? timeProvider = null,
        string? dataDir = null,
        IPlatformServices? platformServices = null)
    {
        _shutdownAction = shutdownAction;
        _timeProvider = timeProvider ?? TimeProvider.System;
        var provider = _timeProvider;

        DataDirectory = dataDir ?? AppDataPaths.GetDataDirectory();
        Store = new AppDataStore(DataDirectory, provider);

        var settings = Store.LoadSettings();

        if (platformServices != null)
        {
            PlatformServices = platformServices;
        }
        else if (OperatingSystem.IsWindows())
        {
            var exePath = Environment.ProcessPath ?? "Ocy.exe";
            PlatformServices = new WindowsPlatformServices(exePath);
        }
        else
        {
            PlatformServices = UnsupportedPlatformServices.Instance;
        }

        Engine = new BreakEngine(
            settings: settings,
            platformServices: PlatformServices,
            timeProvider: provider,
            dataStore: Store);

        Strings = new Strings();
        TrayPresenter = new TrayPresenter(Strings, provider);

        BreakWindows = breakWindows ?? new DesktopBreakWindows(Engine, Strings, platformServices: PlatformServices);
        SettingsWindow = settingsWindow ?? new DesktopSettingsWindow(this);

        Coordinator = new BreakUiCoordinator(Engine, BreakWindows, SettingsWindow);

        PlatformServices.Session.SessionChanged += OnSessionChanged;
        PlatformServices.Session.Start();

        Engine.PhaseChanged += OnPhaseChanged;

        _timer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
        _timer.Tick += OnTick;
    }

    public void AttachTray(Action quitAction)
    {
        TrayController = new TrayController(
            Engine,
            Coordinator,
            TrayPresenter,
            quitAction,
            PlatformServices.TaskbarTheme,
            _timeProvider);
    }

    public void Start()
    {
        _timer.Start();
    }

    private void OnSessionChanged(object? sender, SessionEventKind kind)
    {
        Dispatcher.UIThread.Post(() =>
        {
            if (!_disposed)
            {
                Engine.OnSessionEvent(kind);
            }
        });
    }

    private void OnPhaseChanged(EngineSnapshot snapshot)
    {
        Dispatcher.UIThread.Post(() =>
        {
            if (!_disposed)
            {
                TrayController?.Update(snapshot);
            }
        });
    }

    private void OnTick(object? sender, EventArgs e)
    {
        if (_disposed)
        {
            return;
        }

        Engine.Tick();
        var snapshot = Engine.Snapshot;
        Coordinator.OnTick(snapshot);
        TrayController?.Update(snapshot);

        _secondsSinceSave++;
        if (_secondsSinceSave >= 60)
        {
            _secondsSinceSave = 0;
            try
            {
                Engine.Save();
            }
            catch (Exception ex)
            {
                System.Diagnostics.Trace.TraceError($"AppHost: periodic save failed: {ex}");
                ErrorLogger.Log(DataDirectory, "AppHost: periodic save failed", ex);
            }
        }
    }

    public bool SaveSettings(OcySettings newSettings)
    {
        ArgumentNullException.ThrowIfNull(newSettings);
        try
        {
            Engine.UpdateSettings(newSettings);
            Store.SaveSettings(newSettings);
            Engine.Save();
            return true;
        }
        catch (Exception ex)
        {
            System.Diagnostics.Trace.TraceError($"AppHost: saving settings failed: {ex}");
            ErrorLogger.Log(DataDirectory, "AppHost: saving settings failed", ex);
            return false;
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _timer.Stop();

        Engine.PhaseChanged -= OnPhaseChanged;
        PlatformServices.Session.SessionChanged -= OnSessionChanged;

        try
        {
            Engine.Dispose();
        }
        catch (Exception ex)
        {
            System.Diagnostics.Trace.TraceError($"AppHost: exit dispose failed: {ex}");
            ErrorLogger.Log(DataDirectory, "AppHost: exit dispose failed", ex);
        }

        TrayController?.Dispose();
        Coordinator.Dispose();
        PlatformServices.Dispose();
    }
}
