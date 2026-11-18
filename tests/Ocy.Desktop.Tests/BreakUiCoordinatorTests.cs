using Microsoft.Extensions.Time.Testing;
using Ocy.Core.Engine;
using Ocy.Core.Exercises;
using Ocy.Core.Platform;
using Ocy.Core.Settings;
using Ocy.Desktop.Coordinating;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class BreakUiCoordinatorTests : IDisposable
{
    private readonly FakeTimeProvider _time;
    private readonly OcySettings _settings;
    private readonly BreakEngine _engine;
    private readonly RecordingBreakWindows _breakWindows;
    private readonly RecordingSettingsWindow _settingsWindow;
    private readonly BreakUiCoordinator _coordinator;

    public BreakUiCoordinatorTests()
    {
        _time = new FakeTimeProvider(new DateTimeOffset(2026, 11, 17, 9, 0, 0, TimeSpan.Zero));
        _settings = new OcySettings();
        _settings.ShortBreaks.IntervalMinutes = 10;
        _settings.ShortBreaks.DurationSeconds = 15;
        _settings.LongBreaks.IntervalMinutes = 60;
        _settings.LongBreaks.DurationMinutes = 5;
        _settings.LongBreaks.WarningSeconds = 60;
        _settings.LongBreaks.PostponeMinutes = 3;
        _settings.LongBreaks.MaxPostpones = 3;
        _settings.AwayThresholdSeconds = 120;

        _engine = new BreakEngine(
            settings: _settings,
            platformServices: UnsupportedPlatformServices.Instance,
            timeProvider: _time,
            exercisePicker: new ExercisePicker(new Random(42)));

        _breakWindows = new RecordingBreakWindows();
        _settingsWindow = new RecordingSettingsWindow();

        _coordinator = new BreakUiCoordinator(_engine, _breakWindows, _settingsWindow);
    }

    public void Dispose()
    {
        _coordinator.Dispose();
        _engine.Dispose();
    }

    [Fact]
    public void Flow1_ShortBreak_ShowsThenCloses()
    {
        // 1. Advance time until short break triggers
        for (var i = 0; i < 600; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
            _coordinator.OnTick();
        }

        Assert.Equal(EnginePhase.ShortBreak, _engine.Phase);
        Assert.Single(_breakWindows.ShownShortBreaks);
        Assert.NotEmpty(_breakWindows.CountdownUpdates);
        Assert.Equal(0, _breakWindows.CloseAllCallCount);

        // 2. Advance time until short break finishes (15 seconds)
        for (var i = 0; i < 15; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
            _coordinator.OnTick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.True(_breakWindows.CloseAllCallCount >= 1);
    }

    [Fact]
    public void Flow2_Warning_To_LongBreak()
    {
        // Disable short breaks to isolate long break flow
        _settings.ShortBreaks.Enabled = false;
        _engine.UpdateSettings(_settings);

        // Advance time to 59 minutes: warning begins
        for (var i = 0; i < 59 * 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
            _coordinator.OnTick();
        }

        Assert.Equal(EnginePhase.LongBreakWarning, _engine.Phase);
        Assert.Single(_breakWindows.ShownWarnings);
        Assert.Equal(TimeSpan.FromSeconds(60), _breakWindows.ShownWarnings[0]);

        // Advance 60 seconds of warning countdown
        for (var i = 0; i < 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
            _coordinator.OnTick();
        }

        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.Single(_breakWindows.ShownLongBreaks);
    }

    [Fact]
    public void Flow3_Pause_ClosesEverything()
    {
        // Start a short break
        var started = _engine.StartShortBreakNow();
        Assert.True(started);
        Assert.Equal(EnginePhase.ShortBreak, _engine.Phase);
        Assert.Single(_breakWindows.ShownShortBreaks);

        var closeCountBefore = _breakWindows.CloseAllCallCount;

        // Pause the engine
        _engine.PauseFor(TimeSpan.FromHours(1));

        Assert.Equal(EnginePhase.Paused, _engine.Phase);
        Assert.True(_breakWindows.CloseAllCallCount > closeCountBefore);
    }

    [Fact]
    public void Coordinator_ShowSettings_CallsSettingsWindow()
    {
        Assert.Equal(0, _settingsWindow.ShowCount);
        _coordinator.ShowSettings();
        Assert.Equal(1, _settingsWindow.ShowCount);
    }

    private sealed class RecordingBreakWindows : IBreakWindows
    {
        public List<BreakInfo> ShownShortBreaks { get; } = [];
        public List<TimeSpan> ShownWarnings { get; } = [];
        public List<BreakInfo> ShownLongBreaks { get; } = [];
        public List<EngineSnapshot> CountdownUpdates { get; } = [];
        public int CloseAllCallCount { get; private set; }

        public void ShowShortBreak(BreakInfo info) => ShownShortBreaks.Add(info);
        public void ShowWarning(TimeSpan countdown) => ShownWarnings.Add(countdown);
        public void ShowLongBreak(BreakInfo info) => ShownLongBreaks.Add(info);
        public void UpdateCountdown(EngineSnapshot snapshot) => CountdownUpdates.Add(snapshot);
        public void CloseAll() => CloseAllCallCount++;
    }

    private sealed class RecordingSettingsWindow : ISettingsWindow
    {
        public int ShowCount { get; private set; }
        public void Show() => ShowCount++;
    }
}
