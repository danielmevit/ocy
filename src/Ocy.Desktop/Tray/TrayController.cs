using Avalonia;
using Avalonia.Controls;
using Avalonia.Styling;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Desktop.Coordinating;

namespace Ocy.Desktop.Tray;

public sealed class TrayController : IDisposable
{
    private readonly BreakEngine _engine;
    private readonly BreakUiCoordinator _coordinator;
    private readonly TrayPresenter _presenter;
    private readonly Action _quitAction;
    private readonly ITaskbarThemeProvider _taskbarTheme;
    private readonly TimeProvider _timeProvider;

    private readonly TrayIcon _trayIcon;
    private readonly NativeMenu _menu;
    private readonly NativeMenuItem _takeLongBreakItem;
    private readonly NativeMenuItemSeparator _separator1;
    private readonly NativeMenuItem _pause1hItem;
    private readonly NativeMenuItem _pause3hItem;
    private readonly NativeMenuItem _pauseUntilResumedItem;
    private readonly NativeMenuItem _resumeItem;
    private readonly NativeMenuItemSeparator _separator2;
    private readonly NativeMenuItem _settingsItem;
    private readonly NativeMenuItem _quitItem;

    private DateTimeOffset _lastTaskbarThemeCheckUtc;
    private bool? _cachedIsLightTaskbar;
    private bool _useLightSet;

    private TrayIconState? _currentIconState;
    private bool? _currentUseLightSet;

    public TrayIcon TrayIcon => _trayIcon;

    public bool CurrentUseLightSet => _useLightSet;

    public TrayController(
        BreakEngine engine,
        BreakUiCoordinator coordinator,
        TrayPresenter presenter,
        Action quitAction,
        ITaskbarThemeProvider? taskbarTheme = null,
        TimeProvider? timeProvider = null)
    {
        _engine = engine ?? throw new ArgumentNullException(nameof(engine));
        _coordinator = coordinator ?? throw new ArgumentNullException(nameof(coordinator));
        _presenter = presenter ?? throw new ArgumentNullException(nameof(presenter));
        _quitAction = quitAction ?? throw new ArgumentNullException(nameof(quitAction));
        _taskbarTheme = taskbarTheme ?? UnsupportedTaskbarThemeProvider.Instance;
        _timeProvider = timeProvider ?? TimeProvider.System;

        _takeLongBreakItem = new NativeMenuItem();
        _takeLongBreakItem.Click += (_, _) => _engine.StartLongBreakNow();

        _separator1 = new NativeMenuItemSeparator();

        _pause1hItem = new NativeMenuItem();
        _pause1hItem.Click += (_, _) => _engine.PauseFor(TimeSpan.FromHours(1));

        _pause3hItem = new NativeMenuItem();
        _pause3hItem.Click += (_, _) => _engine.PauseFor(TimeSpan.FromHours(3));

        _pauseUntilResumedItem = new NativeMenuItem();
        _pauseUntilResumedItem.Click += (_, _) => _engine.PauseFor(null);

        _resumeItem = new NativeMenuItem();
        _resumeItem.Click += (_, _) => _engine.Resume();

        _separator2 = new NativeMenuItemSeparator();

        _settingsItem = new NativeMenuItem();
        _settingsItem.Click += (_, _) => _coordinator.ShowSettings();

        _quitItem = new NativeMenuItem();
        _quitItem.Click += (_, _) => _quitAction();

        _menu = new NativeMenu
        {
            _takeLongBreakItem,
            _separator1,
            _pause1hItem,
            _pause3hItem,
            _pauseUntilResumedItem,
            _resumeItem,
            _separator2,
            _settingsItem,
            _quitItem,
        };

        _trayIcon = new TrayIcon
        {
            Menu = _menu,
            IsVisible = true,
        };

        if (Application.Current != null)
        {
            Application.Current.ActualThemeVariantChanged += OnThemeVariantChanged;
        }

        ReevaluateSet(forceProviderRead: true);
        Update(_engine.Snapshot);
    }

    public TrayController(
        BreakEngine engine,
        BreakUiCoordinator coordinator,
        TrayPresenter presenter,
        Action quitAction,
        IPlatformServices platformServices,
        TimeProvider? timeProvider = null)
        : this(engine, coordinator, presenter, quitAction, platformServices?.TaskbarTheme, timeProvider)
    {
    }

    private void OnThemeVariantChanged(object? sender, EventArgs e)
    {
        ReevaluateSet(forceProviderRead: true);
        Update(_engine.Snapshot);
    }

    private void ReevaluateSet(bool forceProviderRead)
    {
        var now = _timeProvider.GetUtcNow();
        if (forceProviderRead || now - _lastTaskbarThemeCheckUtc >= TimeSpan.FromSeconds(60))
        {
            _lastTaskbarThemeCheckUtc = now;
            try
            {
                _cachedIsLightTaskbar = _taskbarTheme.IsLightTaskbar();
            }
            catch
            {
                _cachedIsLightTaskbar = null;
            }
        }

        var appTheme = Application.Current?.ActualThemeVariant ?? ThemeVariant.Light;
        _useLightSet = TraySetSelector.UseLightSet(_cachedIsLightTaskbar, appTheme);
    }

    public void Update(EngineSnapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);

        ReevaluateSet(forceProviderRead: false);

        var presentation = _presenter.Present(snapshot);

        _trayIcon.ToolTipText = presentation.Tooltip;

        var newIconState = presentation.IconState;
        if (_currentIconState != newIconState || _currentUseLightSet != _useLightSet)
        {
            _currentIconState = newIconState;
            _currentUseLightSet = _useLightSet;
            _trayIcon.Icon = TrayIconRenderer.GetIcon(newIconState, _useLightSet);
        }

        _takeLongBreakItem.Header = presentation.TakeLongBreakNow.Text;
        _takeLongBreakItem.IsVisible = presentation.TakeLongBreakNow.IsVisible;
        _takeLongBreakItem.IsEnabled = presentation.TakeLongBreakNow.IsEnabled;

        _pause1hItem.Header = presentation.Pause1Hour.Text;
        _pause1hItem.IsVisible = presentation.Pause1Hour.IsVisible;
        _pause1hItem.IsEnabled = presentation.Pause1Hour.IsEnabled;

        _pause3hItem.Header = presentation.Pause3Hours.Text;
        _pause3hItem.IsVisible = presentation.Pause3Hours.IsVisible;
        _pause3hItem.IsEnabled = presentation.Pause3Hours.IsEnabled;

        _pauseUntilResumedItem.Header = presentation.PauseUntilResumed.Text;
        _pauseUntilResumedItem.IsVisible = presentation.PauseUntilResumed.IsVisible;
        _pauseUntilResumedItem.IsEnabled = presentation.PauseUntilResumed.IsEnabled;

        _resumeItem.Header = presentation.Resume.Text;
        _resumeItem.IsVisible = presentation.Resume.IsVisible;
        _resumeItem.IsEnabled = presentation.Resume.IsEnabled;

        _settingsItem.Header = presentation.Settings.Text;
        _settingsItem.IsVisible = presentation.Settings.IsVisible;
        _settingsItem.IsEnabled = presentation.Settings.IsEnabled;

        _quitItem.Header = presentation.Quit.Text;
        _quitItem.IsVisible = presentation.Quit.IsVisible;
        _quitItem.IsEnabled = presentation.Quit.IsEnabled;
    }

    public void Dispose()
    {
        if (Application.Current != null)
        {
            Application.Current.ActualThemeVariantChanged -= OnThemeVariantChanged;
        }

        _trayIcon.IsVisible = false;
        _trayIcon.Dispose();
    }
}
