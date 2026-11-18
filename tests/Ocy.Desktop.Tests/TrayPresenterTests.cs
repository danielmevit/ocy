using Microsoft.Extensions.Time.Testing;
using Ocy.Core.Engine;
using Ocy.Desktop.Localization;
using Ocy.Desktop.Tray;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class TrayPresenterTests
{
    private readonly Strings _strings = new();
    private readonly FakeTimeProvider _timeProvider = new(new DateTimeOffset(2026, 11, 17, 10, 0, 0, TimeSpan.Zero));
    private readonly TrayPresenter _presenter;

    public TrayPresenterTests()
    {
        _presenter = new TrayPresenter(_strings, _timeProvider);
    }

    [Fact]
    public void WorkingPhase_PresentsCorrectState()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.Working,
            TimeUntilShortBreak: TimeSpan.FromMinutes(8),
            TimeUntilLongBreak: TimeSpan.FromMinutes(45),
            CurrentBreak: null,
            PausedUntilUtc: null,
            PostponesLeft: 3,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);

        Assert.Equal(TrayIconState.Active, presentation.IconState);
        Assert.Equal("Next break in 8 min", presentation.Tooltip);
        Assert.True(presentation.TakeLongBreakNow.IsEnabled);
        Assert.False(presentation.Resume.IsVisible);
        Assert.True(presentation.Quit.IsEnabled);
        Assert.True(presentation.Settings.IsEnabled);
    }

    [Fact]
    public void WarningPhase_PresentsCorrectState()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.LongBreakWarning,
            TimeUntilShortBreak: TimeSpan.FromMinutes(5),
            TimeUntilLongBreak: TimeSpan.FromSeconds(42),
            CurrentBreak: null,
            PausedUntilUtc: null,
            PostponesLeft: 3,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);

        Assert.Equal(TrayIconState.Attention, presentation.IconState);
        Assert.Equal("Next break in 0:42", presentation.Tooltip);
        Assert.True(presentation.TakeLongBreakNow.IsEnabled);
        Assert.False(presentation.Resume.IsVisible);
        Assert.True(presentation.Quit.IsEnabled);
    }

    [Fact]
    public void ShortBreakPhase_PresentsCorrectState()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.ShortBreak,
            TimeUntilShortBreak: TimeSpan.Zero,
            TimeUntilLongBreak: TimeSpan.FromMinutes(30),
            CurrentBreak: new CurrentBreakInfo(BreakKind.Short, "blink", TimeSpan.FromSeconds(15), TimeSpan.FromSeconds(10)),
            PausedUntilUtc: null,
            PostponesLeft: 3,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);

        Assert.Equal(TrayIconState.OnBreak, presentation.IconState);
        Assert.Equal("On a break", presentation.Tooltip);
        Assert.False(presentation.TakeLongBreakNow.IsEnabled);
        Assert.False(presentation.Resume.IsVisible);
        Assert.True(presentation.Pause1Hour.IsEnabled);
        Assert.True(presentation.Quit.IsEnabled);
    }

    [Fact]
    public void NonStrictLongBreak_PresentsCorrectState()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.LongBreak,
            TimeUntilShortBreak: null,
            TimeUntilLongBreak: TimeSpan.Zero,
            CurrentBreak: new CurrentBreakInfo(BreakKind.Long, "palming", TimeSpan.FromMinutes(5), TimeSpan.FromMinutes(4)),
            PausedUntilUtc: null,
            PostponesLeft: 0,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);

        Assert.Equal(TrayIconState.OnBreak, presentation.IconState);
        Assert.Equal("On a break", presentation.Tooltip);
        Assert.False(presentation.TakeLongBreakNow.IsEnabled);
        Assert.False(presentation.Resume.IsVisible);
        Assert.True(presentation.Pause1Hour.IsEnabled);
        Assert.True(presentation.Pause3Hours.IsEnabled);
        Assert.True(presentation.PauseUntilResumed.IsEnabled);
        Assert.True(presentation.Settings.IsEnabled);
        Assert.True(presentation.Quit.IsEnabled);
    }

    [Fact]
    public void StrictLongBreak_DisablesAllMenuItemsExceptQuit()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.LongBreak,
            TimeUntilShortBreak: null,
            TimeUntilLongBreak: TimeSpan.Zero,
            CurrentBreak: new CurrentBreakInfo(BreakKind.Long, "palming", TimeSpan.FromMinutes(5), TimeSpan.FromMinutes(4)),
            PausedUntilUtc: null,
            PostponesLeft: 0,
            StrictMode: true);

        var presentation = _presenter.Present(snapshot);

        Assert.Equal(TrayIconState.OnBreak, presentation.IconState);
        Assert.Equal("On a break", presentation.Tooltip);
        Assert.False(presentation.TakeLongBreakNow.IsEnabled);
        Assert.False(presentation.Pause1Hour.IsEnabled);
        Assert.False(presentation.Pause3Hours.IsEnabled);
        Assert.False(presentation.PauseUntilResumed.IsEnabled);
        Assert.False(presentation.Resume.IsEnabled);
        Assert.False(presentation.Settings.IsEnabled);
        Assert.True(presentation.Quit.IsEnabled);
    }

    [Fact]
    public void DeferredPhase_PresentsCorrectState()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.Deferred,
            TimeUntilShortBreak: null,
            TimeUntilLongBreak: TimeSpan.Zero,
            CurrentBreak: null,
            PausedUntilUtc: null,
            PostponesLeft: 3,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);

        Assert.Equal(TrayIconState.Active, presentation.IconState);
        Assert.True(presentation.TakeLongBreakNow.IsEnabled);
        Assert.False(presentation.Resume.IsVisible);
    }

    [Fact]
    public void AwayPhase_PresentsCorrectState()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.Away,
            TimeUntilShortBreak: TimeSpan.FromMinutes(5),
            TimeUntilLongBreak: TimeSpan.FromMinutes(35),
            CurrentBreak: null,
            PausedUntilUtc: null,
            PostponesLeft: 3,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);

        Assert.Equal(TrayIconState.Active, presentation.IconState);
        Assert.False(presentation.TakeLongBreakNow.IsEnabled);
        Assert.False(presentation.Resume.IsVisible);
    }

    [Fact]
    public void PausedPhase_ShowsResumeMenuItem()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.Paused,
            TimeUntilShortBreak: null,
            TimeUntilLongBreak: null,
            CurrentBreak: null,
            PausedUntilUtc: null,
            PostponesLeft: 3,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);

        Assert.Equal(TrayIconState.Paused, presentation.IconState);
        Assert.True(presentation.Resume.IsVisible);
        Assert.True(presentation.Resume.IsEnabled);
        Assert.False(presentation.TakeLongBreakNow.IsEnabled);
    }

    [Fact]
    public void PausedTooltip_IndefinitePause()
    {
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.Paused,
            TimeUntilShortBreak: null,
            TimeUntilLongBreak: null,
            CurrentBreak: null,
            PausedUntilUtc: null,
            PostponesLeft: 3,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);
        Assert.Equal("Paused until you resume", presentation.Tooltip);

        var snapshotMax = snapshot with { PausedUntilUtc = DateTimeOffset.MaxValue };
        var presentationMax = _presenter.Present(snapshotMax);
        Assert.Equal("Paused until you resume", presentationMax.Tooltip);
    }

    [Fact]
    public void PausedTooltip_TimedPause()
    {
        var now = _timeProvider.GetUtcNow();
        var snapshot = new EngineSnapshot(
            Phase: EnginePhase.Paused,
            TimeUntilShortBreak: null,
            TimeUntilLongBreak: null,
            CurrentBreak: null,
            PausedUntilUtc: now + TimeSpan.FromMinutes(23),
            PostponesLeft: 3,
            StrictMode: false);

        var presentation = _presenter.Present(snapshot);
        Assert.Equal("Paused for 23 min more", presentation.Tooltip);

        var snapshotSeconds = snapshot with { PausedUntilUtc = now + TimeSpan.FromSeconds(42) };
        var presentationSeconds = _presenter.Present(snapshotSeconds);
        Assert.Equal("Paused for 0:42 more", presentationSeconds.Tooltip);
    }
}
