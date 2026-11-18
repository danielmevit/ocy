using Ocy.Core.Engine;

namespace Ocy.Desktop.Coordinating;

public sealed class BreakUiCoordinator : IDisposable
{
    private readonly BreakEngine _engine;
    private readonly IBreakWindows _breakWindows;
    private readonly ISettingsWindow _settingsWindow;
    private bool _disposed;

    public IBreakWindows BreakWindows => _breakWindows;

    public ISettingsWindow SettingsWindow => _settingsWindow;

    public BreakUiCoordinator(
        BreakEngine engine,
        IBreakWindows breakWindows,
        ISettingsWindow settingsWindow)
    {
        _engine = engine ?? throw new ArgumentNullException(nameof(engine));
        _breakWindows = breakWindows ?? throw new ArgumentNullException(nameof(breakWindows));
        _settingsWindow = settingsWindow ?? throw new ArgumentNullException(nameof(settingsWindow));

        _engine.BreakStarted += OnBreakStarted;
        _engine.BreakEnded += OnBreakEnded;
        _engine.LongBreakWarningStarted += OnLongBreakWarningStarted;
        _engine.PhaseChanged += OnPhaseChanged;
    }

    public void OnTick()
    {
        OnTick(_engine.Snapshot);
    }

    public void OnTick(EngineSnapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);

        if (snapshot.Phase is EnginePhase.ShortBreak or EnginePhase.LongBreak or EnginePhase.LongBreakWarning)
        {
            _breakWindows.UpdateCountdown(snapshot);
        }
    }

    public void ShowSettings()
    {
        _settingsWindow.Show();
    }

    private void OnBreakStarted(BreakInfo info)
    {
        if (info.Kind == BreakKind.Short)
        {
            _breakWindows.ShowShortBreak(info);
        }
        else if (info.Kind == BreakKind.Long)
        {
            _breakWindows.ShowLongBreak(info);
        }
    }

    private void OnLongBreakWarningStarted(TimeSpan countdown)
    {
        _breakWindows.ShowWarning(countdown);
    }

    private void OnBreakEnded(BreakInfo info, BreakOutcome outcome)
    {
        _breakWindows.CloseAll();
    }

    private void OnPhaseChanged(EngineSnapshot snapshot)
    {
        if (snapshot.Phase is EnginePhase.Working or EnginePhase.Paused or EnginePhase.Away)
        {
            _breakWindows.CloseAll();
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _engine.BreakStarted -= OnBreakStarted;
        _engine.BreakEnded -= OnBreakEnded;
        _engine.LongBreakWarningStarted -= OnLongBreakWarningStarted;
        _engine.PhaseChanged -= OnPhaseChanged;
    }
}
