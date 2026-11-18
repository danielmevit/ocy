using Ocy.Core.Engine;
using Ocy.Desktop.Localization;

namespace Ocy.Desktop.Tray;

public enum TrayIconState
{
    Active,
    Paused,
    OnBreak,
    Attention,
}

public sealed record TrayMenuItemState(string Text, bool IsVisible, bool IsEnabled);

public sealed record TrayPresentation(
    string Tooltip,
    TrayIconState IconState,
    TrayMenuItemState TakeLongBreakNow,
    TrayMenuItemState Pause1Hour,
    TrayMenuItemState Pause3Hours,
    TrayMenuItemState PauseUntilResumed,
    TrayMenuItemState Resume,
    TrayMenuItemState Settings,
    TrayMenuItemState Quit);

public sealed class TrayPresenter
{
    private readonly Strings _strings;
    private readonly TimeProvider _timeProvider;

    public TrayPresenter(Strings strings, TimeProvider? timeProvider = null)
    {
        _strings = strings ?? throw new ArgumentNullException(nameof(strings));
        _timeProvider = timeProvider ?? TimeProvider.System;
    }

    public TrayPresentation Present(EngineSnapshot snapshot, DateTimeOffset? now = null)
    {
        ArgumentNullException.ThrowIfNull(snapshot);

        var currentUtc = now ?? _timeProvider.GetUtcNow();
        var iconState = ComputeIconState(snapshot.Phase);
        var tooltip = ComputeTooltip(snapshot, currentUtc);

        var isStrictLongBreak = snapshot.Phase == EnginePhase.LongBreak && snapshot.StrictMode;
        var isPaused = snapshot.Phase == EnginePhase.Paused;

        var takeLongBreakText = _strings.Get("tray.menu.takeLongBreak");
        var pause1hText = _strings.Get("tray.menu.pause1h");
        var pause3hText = _strings.Get("tray.menu.pause3h");
        var pauseUntilResumedText = _strings.Get("tray.menu.pauseUntilResumed");
        var resumeText = _strings.Get("tray.menu.resume");
        var settingsText = _strings.Get("tray.menu.settings");
        var quitText = _strings.Get("tray.menu.quit");

        if (isStrictLongBreak)
        {
            return new TrayPresentation(
                Tooltip: tooltip,
                IconState: iconState,
                TakeLongBreakNow: new TrayMenuItemState(takeLongBreakText, IsVisible: true, IsEnabled: false),
                Pause1Hour: new TrayMenuItemState(pause1hText, IsVisible: true, IsEnabled: false),
                Pause3Hours: new TrayMenuItemState(pause3hText, IsVisible: true, IsEnabled: false),
                PauseUntilResumed: new TrayMenuItemState(pauseUntilResumedText, IsVisible: true, IsEnabled: false),
                Resume: new TrayMenuItemState(resumeText, IsVisible: false, IsEnabled: false),
                Settings: new TrayMenuItemState(settingsText, IsVisible: true, IsEnabled: false),
                Quit: new TrayMenuItemState(quitText, IsVisible: true, IsEnabled: true));
        }

        var canStartLongBreak = snapshot.Phase is EnginePhase.Working or EnginePhase.LongBreakWarning or EnginePhase.Deferred;

        return new TrayPresentation(
            Tooltip: tooltip,
            IconState: iconState,
            TakeLongBreakNow: new TrayMenuItemState(takeLongBreakText, IsVisible: true, IsEnabled: canStartLongBreak),
            Pause1Hour: new TrayMenuItemState(pause1hText, IsVisible: true, IsEnabled: true),
            Pause3Hours: new TrayMenuItemState(pause3hText, IsVisible: true, IsEnabled: true),
            PauseUntilResumed: new TrayMenuItemState(pauseUntilResumedText, IsVisible: true, IsEnabled: true),
            Resume: new TrayMenuItemState(resumeText, IsVisible: isPaused, IsEnabled: isPaused),
            Settings: new TrayMenuItemState(settingsText, IsVisible: true, IsEnabled: true),
            Quit: new TrayMenuItemState(quitText, IsVisible: true, IsEnabled: true));
    }

    private static TrayIconState ComputeIconState(EnginePhase phase)
    {
        return phase switch
        {
            EnginePhase.LongBreakWarning => TrayIconState.Attention,
            EnginePhase.Paused => TrayIconState.Paused,
            EnginePhase.ShortBreak or EnginePhase.LongBreak => TrayIconState.OnBreak,
            _ => TrayIconState.Active,
        };
    }

    private string ComputeTooltip(EngineSnapshot snapshot, DateTimeOffset now)
    {
        if (snapshot.Phase is EnginePhase.ShortBreak or EnginePhase.LongBreak)
        {
            return _strings.Get("tray.tooltip.onBreak");
        }

        if (snapshot.Phase == EnginePhase.Paused)
        {
            if (!snapshot.PausedUntilUtc.HasValue || snapshot.PausedUntilUtc.Value == DateTimeOffset.MaxValue)
            {
                return _strings.Get("tray.tooltip.pausedIndefinitely");
            }

            var remaining = snapshot.PausedUntilUtc.Value - now;
            if (remaining < TimeSpan.Zero)
            {
                remaining = TimeSpan.Zero;
            }

            return _strings.Format("tray.tooltip.paused", ("time", _strings.FormatTime(remaining)));
        }

        TimeSpan? nextBreak = null;
        if (snapshot.TimeUntilShortBreak.HasValue && snapshot.TimeUntilLongBreak.HasValue)
        {
            nextBreak = snapshot.TimeUntilShortBreak.Value < snapshot.TimeUntilLongBreak.Value
                ? snapshot.TimeUntilShortBreak.Value
                : snapshot.TimeUntilLongBreak.Value;
        }
        else if (snapshot.TimeUntilShortBreak.HasValue)
        {
            nextBreak = snapshot.TimeUntilShortBreak.Value;
        }
        else if (snapshot.TimeUntilLongBreak.HasValue)
        {
            nextBreak = snapshot.TimeUntilLongBreak.Value;
        }

        if (nextBreak.HasValue)
        {
            return _strings.Format("tray.tooltip.working", ("time", _strings.FormatTime(nextBreak.Value)));
        }

        return _strings.Get("app.name");
    }
}
