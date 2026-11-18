using System.Reflection;
using Avalonia.Controls;
using Avalonia.Threading;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Settings;
using Ocy.Desktop.Controls;
using Ocy.Desktop.Host;
using Ocy.Desktop.Localization;
using Ocy.Desktop.Tray;

namespace Ocy.Desktop.Windows;

/// <summary>
/// Settings window. Static text comes from <c>{loc:Str key}</c> in XAML; controls are the generated
/// <c>x:Name</c> fields. Every change is applied live (engine + settings.json), debounced by
/// <see cref="ApplyDelay"/>; closing the window applies any pending change at once.
/// </summary>
public partial class SettingsWindow : Window
{
    /// <summary>How long the window waits after the last change before applying it.</summary>
    public static readonly TimeSpan ApplyDelay = TimeSpan.FromMilliseconds(400);

    private readonly AppHost? _host;
    private readonly BreakEngine? _engine;
    private readonly IPlatformServices? _platform;
    private readonly Strings _strings;
    private readonly DispatcherTimer _applyTimer;
    private bool _loading;
    private bool _suppressAutostartEvents;

    /// <summary>True while a change waits for the debounce to apply it.</summary>
    public bool HasPendingChanges { get; private set; }

    public SettingsWindow()
    {
        InitializeComponent();
        _strings = Strings.Shared;

        _applyTimer = new DispatcherTimer { Interval = ApplyDelay };
        _applyTimer.Tick += (_, _) => FlushPendingChanges();
        Closing += (_, _) => FlushPendingChanges();
        Closed += (_, _) => _applyTimer.Stop();

        ApplyTexts();
    }

    public SettingsWindow(AppHost host)
        : this()
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
        _engine = host.Engine;
        _platform = host.PlatformServices;
        _strings = host.Strings;

        ApplyTexts();
        LoadFrom(_engine.Settings);
        WireEvents();
    }

    private IEnumerable<ToggleSwitch> SettingToggles =>
    [
        SoundsToggle, StatusNotificationsToggle,
        ShortBreaksEnabledToggle, ShortFullscreenToggle,
        LongBreaksEnabledToggle,
        StrictModeToggle, InactivityTrackingToggle, PauseFullscreenToggle, HasWindowNearbyToggle,
    ];

    private IEnumerable<NumericUpDown> NumericInputs =>
    [
        ShortIntervalInput, ShortDurationInput,
        LongIntervalInput, LongDurationInput, LongWarningInput, LongPostponeInput, LongMaxPostponesInput,
        AwayThresholdInput,
    ];

    /// <summary>The few texts that are formatted at runtime; everything else is bound in XAML.</summary>
    private void ApplyTexts()
    {
        Icon = AppIcon.GetIcon();
        Title = _strings.Format("settings.windowTitle", ("app", _strings.Get("app.name")));
        AboutAppNameVersion.Text = _strings.Format(
            "about.version",
            ("name", _strings.Get("app.name")),
            ("version", GetAppVersion()));
    }

    internal static string GetAppVersion()
    {
        var assembly = typeof(SettingsWindow).Assembly;
        var informational = assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion
            ?? assembly.GetName().Version?.ToString(3)
            ?? "0.1.0";
        return informational.Split('+')[0];
    }

    internal void LoadFrom(OcySettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        _loading = true;
        _suppressAutostartEvents = true;
        try
        {
            var autostartSupported = _platform?.Autostart.IsSupported ?? true;
            StartWithSystemRow.IsVisible = autostartSupported;
            StartWithSystemToggle.IsChecked = autostartSupported && _platform != null
                ? _platform.Autostart.IsEnabled()
                : settings.StartWithSystem;

            LanguageCombo.ItemsSource = new[] { "en" };
            LanguageCombo.SelectedItem = settings.Language ?? "en";
            SoundsToggle.IsChecked = settings.Sounds;
            StatusNotificationsToggle.IsChecked = settings.StatusNotifications;

            ShortBreaksEnabledToggle.IsChecked = settings.ShortBreaks.Enabled;
            ShortIntervalInput.Value = settings.ShortBreaks.IntervalMinutes;
            ShortDurationInput.Value = settings.ShortBreaks.DurationSeconds;
            ShortFullscreenToggle.IsChecked = settings.ShortBreaks.Fullscreen;

            LongBreaksEnabledToggle.IsChecked = settings.LongBreaks.Enabled;
            LongIntervalInput.Value = settings.LongBreaks.IntervalMinutes;
            LongDurationInput.Value = settings.LongBreaks.DurationMinutes;
            LongWarningInput.Value = settings.LongBreaks.WarningSeconds;
            LongPostponeInput.Value = settings.LongBreaks.PostponeMinutes;
            LongMaxPostponesInput.Value = settings.LongBreaks.MaxPostpones;

            StrictModeToggle.IsChecked = settings.StrictMode;
            InactivityTrackingToggle.IsChecked = settings.InactivityTracking;
            AwayThresholdInput.Value = settings.AwayThresholdSeconds;
            PauseFullscreenToggle.IsChecked = settings.PauseDuringFullscreen;
            HasWindowNearbyToggle.IsChecked = settings.HasWindowNearby;
        }
        finally
        {
            _suppressAutostartEvents = false;
            _loading = false;
        }

        UpdateSectionStates();
    }

    /// <summary>
    /// Disables (never hides) the dependent rows of a switched-off section, collapses the General card when it has
    /// no visible rows, and places the row dividers.
    /// </summary>
    private void UpdateSectionStates()
    {
        ShortBreakRows.IsEnabled = ShortBreaksEnabledToggle.IsChecked == true;
        LongBreakRows.IsEnabled = LongBreaksEnabledToggle.IsChecked == true;

        GeneralSectionCard.IsVisible = GeneralRows.Children.OfType<SettingRow>().Any(r => r.IsVisible);

        foreach (var rows in new Panel[] { GeneralRows, ShortBreakRows, LongBreakRows, BehaviourRows })
        {
            SettingRow.UpdateDividers(rows);
        }
    }

    private void WireEvents()
    {
        if (_engine == null || _platform == null || _host == null)
        {
            return;
        }

        foreach (var toggle in SettingToggles)
        {
            toggle.IsCheckedChanged += (_, _) => ScheduleApply();
        }

        foreach (var input in NumericInputs)
        {
            input.ValueChanged += (_, _) => ScheduleApply();
        }

        LanguageCombo.SelectionChanged += (_, _) => ScheduleApply();

        ShortBreaksEnabledToggle.IsCheckedChanged += (_, _) => UpdateSectionStates();
        LongBreaksEnabledToggle.IsCheckedChanged += (_, _) => UpdateSectionStates();

        StartWithSystemToggle.IsCheckedChanged += (_, _) => OnStartWithSystemChanged();

        TryShortBreakButton.Click += (_, _) =>
        {
            FlushPendingChanges();
            _engine.StartShortBreakNow();
        };

        TryLongBreakButton.Click += (_, _) =>
        {
            FlushPendingChanges();
            _engine.StartLongBreakNow();
        };
    }

    private void OnStartWithSystemChanged()
    {
        if (_suppressAutostartEvents || _platform == null)
        {
            return;
        }

        var enabled = StartWithSystemToggle.IsChecked == true;
        try
        {
            _platform.Autostart.SetEnabled(enabled);
            StartWithSystemError.IsVisible = false;
            ScheduleApply();
        }
        catch (Exception)
        {
            StartWithSystemError.Text = _strings.Get("settings.autostart.failed");
            StartWithSystemError.IsVisible = true;
            _suppressAutostartEvents = true;
            try
            {
                StartWithSystemToggle.IsChecked = !enabled;
            }
            finally
            {
                _suppressAutostartEvents = false;
            }
        }
    }

    private void ScheduleApply()
    {
        if (_loading || _host == null)
        {
            return;
        }

        HasPendingChanges = true;
        _applyTimer.Stop();
        _applyTimer.Start();
    }

    /// <summary>Applies a pending (debounced) change now. Used on close, before Try, and by tests.</summary>
    public void FlushPendingChanges()
    {
        _applyTimer.Stop();
        if (!HasPendingChanges)
        {
            return;
        }

        HasPendingChanges = false;
        SaveSettings();
    }

    /// <summary>Applies the current values: engine.UpdateSettings + settings.json. Shows an inline error on failure.</summary>
    public void SaveSettings()
    {
        if (_host == null)
        {
            return;
        }

        // Phase 2: Sounds, StatusNotifications, and Language are kept in the model while their UI rows are hidden in Phase 1
        var updated = _engine?.Settings.Clone() ?? new OcySettings();
        updated.Sounds = SoundsToggle.IsChecked == true;
        updated.StatusNotifications = StatusNotificationsToggle.IsChecked == true;
        updated.Language = LanguageCombo.SelectedItem?.ToString() ?? "en";

        updated.ShortBreaks.Enabled = ShortBreaksEnabledToggle.IsChecked == true;
        updated.ShortBreaks.IntervalMinutes = (int)(ShortIntervalInput.Value ?? 10);
        updated.ShortBreaks.DurationSeconds = (int)(ShortDurationInput.Value ?? 15);
        updated.ShortBreaks.Fullscreen = ShortFullscreenToggle.IsChecked == true;

        updated.LongBreaks.Enabled = LongBreaksEnabledToggle.IsChecked == true;
        updated.LongBreaks.IntervalMinutes = (int)(LongIntervalInput.Value ?? 60);
        updated.LongBreaks.DurationMinutes = (int)(LongDurationInput.Value ?? 5);
        updated.LongBreaks.WarningSeconds = (int)(LongWarningInput.Value ?? 60);
        updated.LongBreaks.PostponeMinutes = (int)(LongPostponeInput.Value ?? 3);
        updated.LongBreaks.MaxPostpones = (int)(LongMaxPostponesInput.Value ?? 3);

        updated.StrictMode = StrictModeToggle.IsChecked == true;
        updated.InactivityTracking = InactivityTrackingToggle.IsChecked == true;
        updated.AwayThresholdSeconds = (int)(AwayThresholdInput.Value ?? 120);
        updated.PauseDuringFullscreen = PauseFullscreenToggle.IsChecked == true;
        updated.HasWindowNearby = HasWindowNearbyToggle.IsChecked == true;

        if (_platform != null && _platform.Autostart.IsSupported)
        {
            updated.StartWithSystem = StartWithSystemToggle.IsChecked == true;
        }
        else if (_engine != null)
        {
            updated.StartWithSystem = _engine.Settings.StartWithSystem;
        }

        SaveErrorText.IsVisible = !_host.SaveSettings(updated);
    }
}
