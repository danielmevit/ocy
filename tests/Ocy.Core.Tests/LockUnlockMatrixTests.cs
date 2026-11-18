using Microsoft.Extensions.Time.Testing;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Exercises;
using Ocy.Core.Settings;
using Ocy.Core.Tests.TestDoubles;
using Xunit;

namespace Ocy.Core.Tests;

public sealed class LockUnlockMatrixTests
{
    private sealed class MatrixFixture : IDisposable
    {
        public FakeTimeProvider Time { get; }
        public TestPlatformServices Platform { get; }
        public OcySettings Settings { get; }
        public BreakEngine Engine { get; }
        public static DateOnly Today => new(2026, 11, 17);

        public int AbsenceCreditedCount { get; private set; }
        public BreakKind? LastAbsenceCreditedKind { get; private set; }
        public int BreakStartedCount { get; private set; }
        public int BreakEndedCount { get; private set; }
        public int ShortBreakEndedCompletedCount { get; private set; }
        public int LongBreakEndedCompletedCount { get; private set; }
        public int LongBreakStartedCount { get; private set; }
        public int LongBreakWarningStartedCount { get; private set; }
        public List<string> EventLog { get; } = new();

        public MatrixFixture()
        {
            Time = new FakeTimeProvider(new DateTimeOffset(2026, 11, 17, 9, 0, 0, TimeSpan.Zero));
            Platform = new TestPlatformServices();

            // Default settings (short every 10 min / 15 s, long every 60 min / 5 min, warning 60 s)
            Settings = new OcySettings();
            Settings.ShortBreaks.IntervalMinutes = 10;
            Settings.ShortBreaks.DurationSeconds = 15;
            Settings.LongBreaks.IntervalMinutes = 60;
            Settings.LongBreaks.DurationMinutes = 5;
            Settings.LongBreaks.WarningSeconds = 60;
            Settings.AwayThresholdSeconds = 120;
            Settings.PauseDuringFullscreen = true;

            Engine = new BreakEngine(Settings, Platform, Time, exercisePicker: new ExercisePicker(new Random(42)));

            Engine.AbsenceCredited += kind =>
            {
                AbsenceCreditedCount++;
                LastAbsenceCreditedKind = kind;
                EventLog.Add($"AbsenceCredited:{kind}");
            };

            Engine.BreakStarted += info =>
            {
                BreakStartedCount++;
                if (info.Kind == BreakKind.Long)
                {
                    LongBreakStartedCount++;
                }
                EventLog.Add($"BreakStarted:{info.Kind}");
            };

            Engine.BreakEnded += (info, outcome) =>
            {
                BreakEndedCount++;
                if (info.Kind == BreakKind.Short && outcome == BreakOutcome.Completed)
                {
                    ShortBreakEndedCompletedCount++;
                }
                else if (info.Kind == BreakKind.Long && outcome == BreakOutcome.Completed)
                {
                    LongBreakEndedCompletedCount++;
                }
                EventLog.Add($"BreakEnded:{info.Kind}:{outcome}");
            };

            Engine.LongBreakWarningStarted += _ =>
            {
                LongBreakWarningStartedCount++;
                EventLog.Add("LongBreakWarningStarted");
            };
        }

        public void TickSeconds(int seconds)
        {
            for (int i = 0; i < seconds; i++)
            {
                Time.Advance(TimeSpan.FromSeconds(1));
                Engine.Tick();
            }
        }

        public void Lock() => Engine.OnSessionEvent(SessionEventKind.Locked);

        public void Unlock() => Engine.OnSessionEvent(SessionEventKind.Unlocked);

        public void Suspend() => Engine.OnSessionEvent(SessionEventKind.Suspending);

        public void Resume() => Engine.OnSessionEvent(SessionEventKind.Resumed);

        public void Dispose()
        {
            Engine.Dispose();
            Platform.Dispose();
        }
    }

    [Fact]
    public void Matrix_Row1_Working_500s_Locked60Min_LandsInWorking_OneAbsenceCredited_CountersReset()
    {
        using var f = new MatrixFixture();

        // 500 s of work
        f.TickSeconds(500);
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);

        // Locked for 60 min (tick every second through the lock)
        f.Lock();
        f.TickSeconds(60 * 60);

        // Unlock, tick once
        f.Unlock();
        f.TickSeconds(1);

        // Expected after unlock: Working, exactly 1 AbsenceCredited, both counters about 0
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);
        Assert.Equal(1, f.AbsenceCreditedCount);
        Assert.Equal(BreakKind.Long, f.LastAbsenceCreditedKind);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceShortBreak);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceLongBreak);
    }

    [Fact]
    public void Matrix_Row2_Working_500s_Locked2Min_LandsInWorking_NoCredit_ShortReset_LongUnchanged()
    {
        using var f = new MatrixFixture();

        // 500 s of work
        f.TickSeconds(500);
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);

        // Locked for 2 min (tick every second through the lock)
        f.Lock();
        f.TickSeconds(2 * 60);

        // Unlock, tick once
        f.Unlock();
        f.TickSeconds(1);

        // Expected after unlock: Working, no credit, short counter reset, long counter unchanged (500 s + 1 s tick)
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);
        Assert.Equal(0, f.AbsenceCreditedCount);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceShortBreak);
        Assert.Equal(TimeSpan.FromSeconds(501), f.Engine.SinceLongBreak);
    }

    [Fact]
    public void Matrix_Row3_LongBreakWarning_Locked60Min_LandsInWorking_OneCredit_NoLongBreakStarted()
    {
        using var f = new MatrixFixture();

        // Work until the warning starts
        while (f.Engine.Phase != EnginePhase.LongBreakWarning)
        {
            f.TickSeconds(1);
        }
        Assert.Equal(EnginePhase.LongBreakWarning, f.Engine.Phase);

        // Locked for 60 min
        f.Lock();
        f.TickSeconds(60 * 60);

        // Unlock, tick once
        f.Unlock();
        f.TickSeconds(1);

        // Expected after unlock: Working, 1 credit, no long break started
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);
        Assert.Equal(1, f.AbsenceCreditedCount);
        Assert.Equal(0, f.LongBreakStartedCount);
        Assert.Equal(0, f.Engine.Statistics.GetOrCreate(MatrixFixture.Today).LongCompleted);
    }

    [Fact]
    public void Matrix_Row4_ShortBreak_Locked60Min_BreakEndsCompleted_ThenCreditsAbsence_LongCounterReset()
    {
        using var f = new MatrixFixture();

        // Work until a short break starts
        while (f.Engine.Phase != EnginePhase.ShortBreak)
        {
            f.TickSeconds(1);
        }
        Assert.Equal(EnginePhase.ShortBreak, f.Engine.Phase);

        // Locked for 60 min
        f.Lock();
        f.TickSeconds(60 * 60);

        // Unlock, tick once
        f.Unlock();
        f.TickSeconds(1);

        // Expected after unlock: 1 BreakEnded(Completed) (short), then 1 AbsenceCredited, long counter about 0
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);
        Assert.Equal(1, f.ShortBreakEndedCompletedCount);
        Assert.Equal(1, f.AbsenceCreditedCount);
        Assert.Equal(BreakKind.Long, f.LastAbsenceCreditedKind);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceLongBreak);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceShortBreak);

        // Event sequence check: break completed first during lock, then absence credited on return
        Assert.Contains("BreakEnded:Short:Completed", f.EventLog);
        Assert.Contains("AbsenceCredited:Long", f.EventLog);
        var breakEndedIndex = f.EventLog.IndexOf("BreakEnded:Short:Completed");
        var absenceIndex = f.EventLog.IndexOf("AbsenceCredited:Long");
        Assert.True(breakEndedIndex < absenceIndex, "BreakEnded(Completed) must precede AbsenceCredited");
    }

    [Fact]
    public void Matrix_Row5_LongBreak_StartNow_Locked60Min_BreakEndsCompleted_Working_LongCompletedOne()
    {
        using var f = new MatrixFixture();

        Assert.True(f.Engine.StartLongBreakNow());
        Assert.Equal(EnginePhase.LongBreak, f.Engine.Phase);

        // Locked for 60 min
        f.Lock();
        f.TickSeconds(60 * 60);

        // Unlock, tick once
        f.Unlock();
        f.TickSeconds(1);

        // Expected after unlock: 1 BreakEnded(Completed) (long), Working, LongCompleted == 1
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);
        Assert.Equal(1, f.LongBreakEndedCompletedCount);
        Assert.Equal(1, f.Engine.Statistics.GetOrCreate(MatrixFixture.Today).LongCompleted);
    }

    [Fact]
    public void Matrix_Row6_LongBreak_StartNow_Locked1Min_StillLongBreak_NoBreakEnded()
    {
        using var f = new MatrixFixture();

        Assert.True(f.Engine.StartLongBreakNow());
        Assert.Equal(EnginePhase.LongBreak, f.Engine.Phase);

        // Locked for 1 min
        f.Lock();
        f.TickSeconds(60);

        // Unlock, tick once
        f.Unlock();
        f.TickSeconds(1);

        // Expected after unlock: still LongBreak, no BreakEnded yet (OC01c behaviour)
        Assert.Equal(EnginePhase.LongBreak, f.Engine.Phase);
        Assert.Equal(0, f.BreakEndedCount);
    }

    [Fact]
    public void Matrix_Row7_Paused_PauseFor3h_Locked60Min_StaysPaused_NoCredit()
    {
        using var f = new MatrixFixture();

        f.Engine.PauseFor(TimeSpan.FromHours(3));
        Assert.Equal(EnginePhase.Paused, f.Engine.Phase);

        // Locked for 60 min
        f.Lock();
        f.TickSeconds(60 * 60);

        // Unlock, tick once
        f.Unlock();
        f.TickSeconds(1);

        // Expected after unlock: Paused, no credit
        Assert.Equal(EnginePhase.Paused, f.Engine.Phase);
        Assert.Equal(0, f.AbsenceCreditedCount);
    }

    [Fact]
    public void Matrix_Row8_Deferred_Fullscreen_Locked60Min_NormalBeforeUnlock_Working_OneCredit_NoWarning()
    {
        using var f = new MatrixFixture();

        // Work until long break is due, with focus = FullscreenApp
        f.TickSeconds(3500);
        f.Platform.Focus.State = FocusState.FullscreenApp;
        while (f.Engine.Phase != EnginePhase.Deferred)
        {
            f.TickSeconds(1);
        }
        Assert.Equal(EnginePhase.Deferred, f.Engine.Phase);

        // Locked for 60 min
        f.Lock();
        f.TickSeconds(60 * 60);

        // Focus back to Normal before unlock
        f.Platform.Focus.State = FocusState.Normal;

        // Unlock, tick once
        f.Unlock();
        f.TickSeconds(1);

        // Expected after unlock: Working, 1 credit, no warning
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);
        Assert.Equal(1, f.AbsenceCreditedCount);
        Assert.Equal(0, f.LongBreakWarningStartedCount);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceLongBreak);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceShortBreak);
    }

    [Fact]
    public void Matrix_Row9_ShortBreak_Suspending60Min_BreakEndsCompleted_ThenCreditsAbsence()
    {
        using var f = new MatrixFixture();

        // Work until a short break starts
        while (f.Engine.Phase != EnginePhase.ShortBreak)
        {
            f.TickSeconds(1);
        }
        Assert.Equal(EnginePhase.ShortBreak, f.Engine.Phase);

        // Suspend for 60 min (tick every second through the lock)
        f.Suspend();
        f.TickSeconds(60 * 60);

        // Resume, tick once
        f.Resume();
        f.TickSeconds(1);

        // Expected after resume: 1 BreakEnded(Completed) (short), then 1 AbsenceCredited, long counter about 0
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);
        Assert.Equal(1, f.ShortBreakEndedCompletedCount);
        Assert.Equal(1, f.AbsenceCreditedCount);
        Assert.Equal(BreakKind.Long, f.LastAbsenceCreditedKind);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceLongBreak);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceShortBreak);

        Assert.Contains("BreakEnded:Short:Completed", f.EventLog);
        Assert.Contains("AbsenceCredited:Long", f.EventLog);
        var breakEndedIndex = f.EventLog.IndexOf("BreakEnded:Short:Completed");
        var absenceIndex = f.EventLog.IndexOf("AbsenceCredited:Long");
        Assert.True(breakEndedIndex < absenceIndex, "BreakEnded(Completed) must precede AbsenceCredited");
    }

    [Fact]
    public void Matrix_FocusNotPresent_ShortBreak60Min_BreakEndsCompleted_ThenCreditsAbsence()
    {
        using var f = new MatrixFixture();

        // Work until a short break starts
        while (f.Engine.Phase != EnginePhase.ShortBreak)
        {
            f.TickSeconds(1);
        }
        Assert.Equal(EnginePhase.ShortBreak, f.Engine.Phase);

        // Focus = NotPresent for 60 min (tick every second)
        f.Platform.Focus.State = FocusState.NotPresent;
        f.TickSeconds(60 * 60);

        // Focus back to Normal, tick once to return from Away
        f.Platform.Focus.State = FocusState.Normal;
        f.TickSeconds(1);

        // Expected on return: 1 BreakEnded(Completed) (short), then 1 AbsenceCredited, Working, counters 0
        Assert.Equal(EnginePhase.Working, f.Engine.Phase);
        Assert.Equal(1, f.ShortBreakEndedCompletedCount);
        Assert.Equal(1, f.AbsenceCreditedCount);
        Assert.Equal(BreakKind.Long, f.LastAbsenceCreditedKind);
        Assert.Equal(TimeSpan.Zero, f.Engine.SinceLongBreak);
        Assert.Equal(TimeSpan.Zero, f.Engine.SinceShortBreak);

        // Active tick in Working accumulates 1 s
        f.TickSeconds(1);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceLongBreak);
        Assert.Equal(TimeSpan.FromSeconds(1), f.Engine.SinceShortBreak);

        Assert.Contains("BreakEnded:Short:Completed", f.EventLog);
        Assert.Contains("AbsenceCredited:Long", f.EventLog);
        var breakEndedIndex = f.EventLog.IndexOf("BreakEnded:Short:Completed");
        var absenceIndex = f.EventLog.IndexOf("AbsenceCredited:Long");
        Assert.True(breakEndedIndex < absenceIndex, "BreakEnded(Completed) must precede AbsenceCredited");
    }
}
