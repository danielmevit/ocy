using Microsoft.Extensions.Time.Testing;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Exercises;
using Ocy.Core.Settings;
using Ocy.Core.Tests.TestDoubles;
using Xunit;

namespace Ocy.Core.Tests;

public sealed class BreakEngineTests : IDisposable
{
    private readonly FakeTimeProvider _time;
    private readonly TestPlatformServices _platform;
    private readonly OcySettings _settings;
    private readonly BreakEngine _engine;

    public void Dispose()
    {
        _engine.Dispose();
        _platform.Dispose();
    }

    public BreakEngineTests()
    {
        _time = new FakeTimeProvider(new DateTimeOffset(2026, 11, 17, 9, 0, 0, TimeSpan.Zero));
        _platform = new TestPlatformServices();
        _settings = new OcySettings();
        _settings.ShortBreaks.IntervalMinutes = 10;
        _settings.ShortBreaks.DurationSeconds = 15;
        _settings.LongBreaks.IntervalMinutes = 60;
        _settings.LongBreaks.DurationMinutes = 5;
        _settings.LongBreaks.WarningSeconds = 60;
        _settings.LongBreaks.PostponeMinutes = 3;
        _settings.LongBreaks.MaxPostpones = 3;
        _settings.AwayThresholdSeconds = 120;
        _settings.PauseDuringFullscreen = true;

        _engine = new BreakEngine(_settings, _platform, _time, exercisePicker: new ExercisePicker(new Random(42)));
    }

    [Fact]
    public void Requirement1_ShortBreak_FiresCompletesAndResetsCounter()
    {
        BreakInfo? startedBreak = null;
        BreakInfo? endedBreak = null;
        BreakOutcome? breakOutcome = null;

        _engine.BreakStarted += info => startedBreak = info;
        _engine.BreakEnded += (info, outcome) =>
        {
            endedBreak = info;
            breakOutcome = outcome;
        };

        // Advance 10 minutes of active time in 1-second ticks
        for (int i = 0; i < 600; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.ShortBreak, _engine.Phase);
        Assert.NotNull(startedBreak);
        Assert.Equal(BreakKind.Short, startedBreak.Kind);
        Assert.Equal(TimeSpan.FromSeconds(15), startedBreak.TotalDuration);

        // Advance 15 seconds of break time
        for (int i = 0; i < 15; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.NotNull(endedBreak);
        Assert.Equal(BreakOutcome.Completed, breakOutcome);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.FromMinutes(10), _engine.SinceLongBreak);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).ShortCompleted);
    }

    [Fact]
    public void Requirement2_LongWarningAndBreak_FiresCompletesAndResetsBothCounters()
    {
        TimeSpan? warningCountdown = null;
        BreakInfo? startedBreak = null;
        BreakOutcome? endedOutcome = null;

        _engine.LongBreakWarningStarted += cd => warningCountdown = cd;
        _engine.BreakStarted += info => startedBreak = info;
        _engine.BreakEnded += (_, outcome) => endedOutcome = outcome;

        // Disable short breaks for this test to isolate long break
        _settings.ShortBreaks.Enabled = false;
        _engine.UpdateSettings(_settings);

        // Advance 59 minutes (interval 60 - warning 1)
        for (int i = 0; i < 59 * 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.LongBreakWarning, _engine.Phase);
        Assert.Equal(TimeSpan.FromSeconds(60), warningCountdown);

        // Advance 60 seconds of warning countdown
        for (int i = 0; i < 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.NotNull(startedBreak);
        Assert.Equal(BreakKind.Long, startedBreak.Kind);
        Assert.Equal(TimeSpan.FromMinutes(5), startedBreak.TotalDuration);

        // Advance 5 minutes of long break
        for (int i = 0; i < 5 * 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(BreakOutcome.Completed, endedOutcome);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).LongCompleted);
    }

    [Fact]
    public void Requirement3_ShortBreak_SuppressedWhenLongBreakDueWithin2Min()
    {
        bool shortBreakStarted = false;
        _engine.BreakStarted += info =>
        {
            if (info.Kind == BreakKind.Short) shortBreakStarted = true;
        };

        // Set engine state where long break is at 58 min 30 sec (due in 1 min 30 sec < 2 min)
        // and short break reaches its 10 minute interval
        var customState = new EngineState
        {
            SinceShortBreak = TimeSpan.FromMinutes(9) + TimeSpan.FromSeconds(59),
            SinceLongBreak = TimeSpan.FromMinutes(58) + TimeSpan.FromSeconds(30),
            LastTickUtc = _time.GetUtcNow(),
        };

        var engine = new BreakEngine(_settings, _platform, _time, initialState: customState);
        engine.BreakStarted += info =>
        {
            if (info.Kind == BreakKind.Short) shortBreakStarted = true;
        };

        _time.Advance(TimeSpan.FromSeconds(1));
        engine.Tick();

        // Short break must NOT have started; counter reset silently
        Assert.False(shortBreakStarted);
        Assert.Equal(TimeSpan.Zero, engine.SinceShortBreak);
    }

    [Fact]
    public void Requirement4_IdleDetectionAndReturnCrediting()
    {
        BreakKind? credited = null;
        _engine.AbsenceCredited += kind => credited = kind;

        // Advance 5 minutes active time
        for (int i = 0; i < 300; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        var sinceShortBefore = _engine.SinceShortBreak;
        var sinceLongBefore = _engine.SinceLongBreak;

        // User goes idle for 120s (threshold)
        _platform.Idle.IdleTime = TimeSpan.FromSeconds(120);
        _time.Advance(TimeSpan.FromSeconds(1));
        _engine.Tick();

        Assert.Equal(EnginePhase.Away, _engine.Phase);

        // Stay away for another 180 seconds (total away = 120 + 180 = 300s = 5 min = long duration)
        _platform.Idle.IdleTime = TimeSpan.FromSeconds(300);
        _time.Advance(TimeSpan.FromSeconds(180));
        _engine.Tick();

        // Counters must NOT have grown while away
        Assert.Equal(EnginePhase.Away, _engine.Phase);

        // User returns: idle drops to 0
        _platform.Idle.IdleTime = TimeSpan.Zero;
        _time.Advance(TimeSpan.FromSeconds(1));
        _engine.Tick();

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(BreakKind.Long, credited);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).AbsencesCredited);
    }

    [Fact]
    public void Requirement4b_IdleReturn_AfterShortDuration_ResetsOnlyShortCounter()
    {
        BreakKind? credited = null;
        _engine.AbsenceCredited += kind => credited = kind;

        // Advance 5 minutes active time
        for (int i = 0; i < 300; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        var sinceLongBefore = _engine.SinceLongBreak;

        // Enter away at threshold (120s)
        _platform.Idle.IdleTime = TimeSpan.FromSeconds(120);
        _time.Advance(TimeSpan.FromSeconds(1));
        _engine.Tick();
        Assert.Equal(EnginePhase.Away, _engine.Phase);

        // Stay away for only 10 more seconds (total away = 130s >= 15s short duration, but < 300s long duration)
        _platform.Idle.IdleTime = TimeSpan.FromSeconds(130);
        _time.Advance(TimeSpan.FromSeconds(10));
        _engine.Tick();

        // User returns
        _platform.Idle.IdleTime = TimeSpan.Zero;
        _time.Advance(TimeSpan.FromSeconds(1));
        _engine.Tick();

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Null(credited);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        // Long counter preserved!
        Assert.Equal(sinceLongBefore, _engine.SinceLongBreak);
    }

    [Fact]
    public void Requirement5_SleepGap_TreatedAsAway()
    {
        // 5 minutes active time
        for (int i = 0; i < 300; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        var sinceLongBefore = _engine.SinceLongBreak;

        // Tick gap dt = 60s > 15s (sleep gap)
        _time.Advance(TimeSpan.FromSeconds(60));
        _engine.Tick();

        // 60s is >= 15s short duration, so short counter resets.
        // But 60s < 300s long duration, so long counter does not reset and dt is NOT added to counters.
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(sinceLongBefore, _engine.SinceLongBreak);
    }

    [Fact]
    public void Requirement6_FullscreenDeferral_ShortDropped_LongWaitsThenWarns()
    {
        _platform.Focus.State = FocusState.FullscreenApp;

        // Advance 10 minutes (short break interval) in fullscreen
        for (int i = 0; i < 600; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        // Short break dropped, phase remains Working (counters reset)
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);

        // Advance until long break warning is due (at 59 minutes)
        // Currently sinceLongBreak is ~10 min. Advance another 49 minutes.
        for (int i = 0; i < 49 * 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        // Now warning becomes due, but focus is FullscreenApp -> enters Deferred!
        Assert.Equal(EnginePhase.Deferred, _engine.Phase);

        // Advance time while deferred and still fullscreen
        for (int i = 0; i < 100; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }
        Assert.Equal(EnginePhase.Deferred, _engine.Phase);

        // Focus returns to Normal
        _platform.Focus.State = FocusState.Normal;
        _time.Advance(TimeSpan.FromSeconds(1));
        _engine.Tick();

        // Enters LongBreakWarning
        Assert.Equal(EnginePhase.LongBreakWarning, _engine.Phase);
    }

    [Fact]
    public void Requirement7_Postpone_EnforcesMaxPostponesAndStrictMode()
    {
        // Disable short breaks to isolate long break postponement
        _settings.ShortBreaks.Enabled = false;
        _engine.UpdateSettings(_settings);

        // Advance to LongBreakWarning
        for (int i = 0; i < 59 * 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }
        Assert.Equal(EnginePhase.LongBreakWarning, _engine.Phase);

        // Postpone 1: success, due again in postponeMinutes (3 min)
        Assert.True(_engine.PostponeLongBreak());
        Assert.Equal(1, _engine.PostponesUsed);
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.FromMinutes(57), _engine.SinceLongBreak); // 60 - 3 = 57 min

        // Advance 2 minutes to warning again (57 + 2 = 59 min)
        for (int i = 0; i < 120; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }
        Assert.Equal(EnginePhase.LongBreakWarning, _engine.Phase);

        // Postpone 2: success
        Assert.True(_engine.PostponeLongBreak());
        Assert.Equal(2, _engine.PostponesUsed);

        // Advance 2 min to warning again
        for (int i = 0; i < 120; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }
        Assert.Equal(EnginePhase.LongBreakWarning, _engine.Phase);

        // Postpone 3: success (max is 3)
        Assert.True(_engine.PostponeLongBreak());
        Assert.Equal(3, _engine.PostponesUsed);

        // Advance 2 min to warning again
        for (int i = 0; i < 120; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }
        Assert.Equal(EnginePhase.LongBreakWarning, _engine.Phase);

        // Postpone 4: refused because maxPostpones reached!
        Assert.False(_engine.PostponeLongBreak());
        Assert.Equal(EnginePhase.LongBreakWarning, _engine.Phase);

        // Test strict mode refusal
        _settings.StrictMode = true;
        _engine.UpdateSettings(_settings);
        Assert.False(_engine.PostponeLongBreak());
    }

    [Fact]
    public void Requirement8_Skip_RefusedForLongInStrictMode_AllowedForShort()
    {
        _settings.StrictMode = true;
        _engine.UpdateSettings(_settings);

        // 1. In ShortBreak: allowed even in strict mode
        _engine.StartShortBreakNow();
        Assert.Equal(EnginePhase.ShortBreak, _engine.Phase);
        Assert.True(_engine.SkipCurrentBreak());
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).ShortSkipped);

        // 2. In LongBreak: refused in strict mode
        _engine.StartLongBreakNow();
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.False(_engine.SkipCurrentBreak());
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);

        // 3. In LongBreak without strict mode: allowed
        _settings.StrictMode = false;
        _engine.UpdateSettings(_settings);
        Assert.True(_engine.SkipCurrentBreak());
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).LongSkipped);
    }

    [Fact]
    public void Requirement9_Pause_ForDurationAndResume()
    {
        // Accumulate active time
        for (int i = 0; i < 300; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        // Pause for 30 minutes
        _engine.PauseFor(TimeSpan.FromMinutes(30));
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        // Advance 10 minutes and tick -> still paused
        _time.Advance(TimeSpan.FromMinutes(10));
        _engine.Tick();
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        // Advance past 30 minutes -> pause ends, counters reset
        _time.Advance(TimeSpan.FromMinutes(21));
        _engine.Tick();
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);

        // Test Pause indefinite (null) and Resume()
        for (int i = 0; i < 100; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }
        _engine.PauseFor(null);
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        _time.Advance(TimeSpan.FromHours(2));
        _engine.Tick();
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        _engine.Resume();
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);
    }

    [Fact]
    public void Requirement10_SessionLockedToUnlocked_CountsAsAway()
    {
        BreakKind? credited = null;
        _engine.AbsenceCredited += kind => credited = kind;

        // Active time
        for (int i = 0; i < 300; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        // Lock session
        _engine.OnSessionEvent(SessionEventKind.Locked);
        Assert.Equal(EnginePhase.Away, _engine.Phase);

        // Advance 10 minutes while locked
        _time.Advance(TimeSpan.FromMinutes(10));

        // Unlock session
        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        // 10 minutes >= 5 min long duration -> credited long break
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(BreakKind.Long, credited);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);
    }

    [Fact]
    public void Requirement16_DisabledBreaks_NeverFire()
    {
        _settings.ShortBreaks.Enabled = false;
        _settings.LongBreaks.Enabled = false;
        _engine.UpdateSettings(_settings);

        bool breakFired = false;
        _engine.BreakStarted += _ => breakFired = true;
        _engine.LongBreakWarningStarted += _ => breakFired = true;

        // Advance 2 hours of ticks
        for (int i = 0; i < 7200; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.False(breakFired);
        Assert.Equal(EnginePhase.Working, _engine.Phase);
    }

    [Fact]
    public void DefectA_ReturnFromAway_IntoFullscreenApp_LandsInWorking_CountersGrow_NoWarningOnNormal()
    {
        // Work 5 min
        for (int i = 0; i < 300; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        // Locked
        _engine.OnSessionEvent(SessionEventKind.Locked);
        Assert.Equal(EnginePhase.Away, _engine.Phase);

        // 30 min later
        _time.Advance(TimeSpan.FromMinutes(30));

        // Set focus = FullscreenApp, then Unlocked
        _platform.Focus.State = FocusState.FullscreenApp;
        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        // Required: ReturnFromAway lands in Working, never in Deferred
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);

        // Counters keep growing in a fullscreen app while nothing is due
        for (int i = 0; i < 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.FromSeconds(60), _engine.SinceLongBreak);

        // Focus returns to Normal
        _platform.Focus.State = FocusState.Normal;
        _time.Advance(TimeSpan.FromSeconds(1));
        _engine.Tick();

        // No warning fires when fullscreen ends
        Assert.Equal(EnginePhase.Working, _engine.Phase);
    }

    [Fact]
    public void DefectB_RestartDuringActivePause_3Hours_StaysPaused()
    {
        _engine.PauseFor(TimeSpan.FromHours(3));
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        var state = _engine.ExportState();

        // Advance 2 minutes later
        _time.Advance(TimeSpan.FromMinutes(2));

        using var engine2 = new BreakEngine(_settings, _platform, _time, initialState: state);
        Assert.Equal(EnginePhase.Paused, engine2.Phase);

        engine2.Tick();
        Assert.Equal(EnginePhase.Paused, engine2.Phase);
        Assert.Equal(state.PausedUntilUtc, engine2.PausedUntilUtc);
    }

    [Fact]
    public void DefectB_RestartDuringActivePause_Indefinite_StaysPaused()
    {
        _engine.PauseFor(null);
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        var state = _engine.ExportState();

        // Advance 10 minutes later
        _time.Advance(TimeSpan.FromMinutes(10));

        using var engine2 = new BreakEngine(_settings, _platform, _time, initialState: state);
        Assert.Equal(EnginePhase.Paused, engine2.Phase);

        engine2.Tick();
        Assert.Equal(EnginePhase.Paused, engine2.Phase);
        Assert.Equal(DateTimeOffset.MaxValue, engine2.PausedUntilUtc);
    }

    [Fact]
    public void DefectB_RestartAfterPauseExpired_TransitionsToWorkingWithCountersReset()
    {
        // Work for 5 minutes so counters are non-zero
        for (int i = 0; i < 300; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.PauseFor(TimeSpan.FromMinutes(10));
        var state = _engine.ExportState();

        // Advance 20 minutes later (pause expired 10 minutes ago)
        _time.Advance(TimeSpan.FromMinutes(20));

        using var engine2 = new BreakEngine(_settings, _platform, _time, initialState: state);
        // Pause expired during downtime: end it normally (EndPause)
        Assert.Equal(EnginePhase.Working, engine2.Phase);
        Assert.Equal(TimeSpan.Zero, engine2.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, engine2.SinceLongBreak);
        Assert.Null(engine2.PausedUntilUtc);

        engine2.Tick();
        Assert.Equal(EnginePhase.Working, engine2.Phase);
    }

    [Fact]
    public void DefectC_StartShortBreakNow_RefusedDuringLongBreakAndPaused_AllowedInWorking()
    {
        // 1. Refused during LongBreak
        Assert.True(_engine.StartLongBreakNow());
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);

        Assert.False(_engine.StartShortBreakNow());
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.Equal(BreakKind.Long, _engine.CurrentBreak?.Kind);

        // 2. Refused during Paused
        _engine.PauseFor(TimeSpan.FromHours(1));
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        Assert.False(_engine.StartShortBreakNow());
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        // 3. Allowed in Working
        _engine.Resume();
        Assert.Equal(EnginePhase.Working, _engine.Phase);

        Assert.True(_engine.StartShortBreakNow());
        Assert.Equal(EnginePhase.ShortBreak, _engine.Phase);
    }

    [Fact]
    public void DefectL1_GapWhileAlreadyAway_CreditsAbsenceOnlyOnce()
    {
        int absenceCreditedCount = 0;
        BreakKind? creditedKind = null;
        _engine.AbsenceCredited += kind =>
        {
            absenceCreditedCount++;
            creditedKind = kind;
        };

        // Work 60 seconds
        for (int i = 0; i < 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        // Idle 3 min -> Away (threshold = 120s)
        _platform.Idle.IdleTime = TimeSpan.FromMinutes(3);
        _time.Advance(TimeSpan.FromSeconds(1));
        _engine.Tick();
        Assert.Equal(EnginePhase.Away, _engine.Phase);
        Assert.Equal(0, absenceCreditedCount);

        // Host stops ticking for 7 min
        _time.Advance(TimeSpan.FromMinutes(7));
        _platform.Idle.IdleTime = TimeSpan.FromMinutes(10);

        // One Tick (gap dt = 7 min > 15s)
        _engine.Tick();
        Assert.Equal(EnginePhase.Away, _engine.Phase);
        Assert.Equal(0, absenceCreditedCount);

        // Then user returns (idle = 0)
        _platform.Idle.IdleTime = TimeSpan.Zero;
        _time.Advance(TimeSpan.FromSeconds(1));
        _engine.Tick();

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(1, absenceCreditedCount);
        Assert.Equal(BreakKind.Long, creditedKind);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).AbsencesCredited);
    }

    [Fact]
    public void DefectI1_PauseFor_DuringShortBreak_RaisesBreakEndedInterrupted_NoStatsIncremented()
    {
        BreakInfo? endedBreak = null;
        BreakOutcome? endedOutcome = null;
        _engine.BreakEnded += (info, outcome) =>
        {
            endedBreak = info;
            endedOutcome = outcome;
        };

        Assert.True(_engine.StartShortBreakNow());
        Assert.Equal(EnginePhase.ShortBreak, _engine.Phase);

        _engine.PauseFor(TimeSpan.FromMinutes(15));

        Assert.Equal(EnginePhase.Paused, _engine.Phase);
        Assert.NotNull(endedBreak);
        Assert.Equal(BreakKind.Short, endedBreak.Kind);
        Assert.Equal(BreakOutcome.Interrupted, endedOutcome);

        var stats = _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17));
        Assert.Equal(0, stats.ShortCompleted);
        Assert.Equal(0, stats.ShortSkipped);
    }

    [Fact]
    public void DefectI1_PauseFor_DuringLongBreak_RaisesBreakEndedInterrupted_NoStatsIncremented()
    {
        BreakInfo? endedBreak = null;
        BreakOutcome? endedOutcome = null;
        _engine.BreakEnded += (info, outcome) =>
        {
            endedBreak = info;
            endedOutcome = outcome;
        };

        Assert.True(_engine.StartLongBreakNow());
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);

        _engine.PauseFor(TimeSpan.FromHours(1));

        Assert.Equal(EnginePhase.Paused, _engine.Phase);
        Assert.NotNull(endedBreak);
        Assert.Equal(BreakKind.Long, endedBreak.Kind);
        Assert.Equal(BreakOutcome.Interrupted, endedOutcome);

        var stats = _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17));
        Assert.Equal(0, stats.LongCompleted);
        Assert.Equal(0, stats.LongSkipped);
    }

    [Fact]
    public void DefectUnlock_UnlockDuringLongBreak_ContinuesAndCompletesNormally()
    {
        int breakEndedCount = 0;
        BreakInfo? endedBreak = null;
        BreakOutcome? endedOutcome = null;
        _engine.BreakEnded += (info, outcome) =>
        {
            breakEndedCount++;
            endedBreak = info;
            endedOutcome = outcome;
        };

        Assert.True(_engine.StartLongBreakNow());
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.NotNull(_engine.CurrentBreak);
        Assert.Equal(TimeSpan.FromMinutes(5), _engine.CurrentBreak.RemainingDuration);

        // Advance 10 s, then lock session
        for (int i = 0; i < 10; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Locked);

        // Advance 60 s while locked
        for (int i = 0; i < 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.NotNull(_engine.CurrentBreak);
        Assert.Equal(TimeSpan.FromSeconds(300 - 70), _engine.CurrentBreak.RemainingDuration);

        // Unlock session during break
        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        // Phase stays LongBreak, remaining keeps counting down, no BreakEnded yet
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.NotNull(_engine.CurrentBreak);
        Assert.Equal(TimeSpan.FromSeconds(230), _engine.CurrentBreak.RemainingDuration);
        Assert.Equal(0, breakEndedCount);

        // Advance the remaining 230 seconds
        for (int i = 0; i < 230; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        // Full duration has elapsed: BreakEnded(Completed) and LongCompleted == 1
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Null(_engine.CurrentBreak);
        Assert.Equal(1, breakEndedCount);
        Assert.NotNull(endedBreak);
        Assert.Equal(BreakKind.Long, endedBreak.Kind);
        Assert.Equal(BreakOutcome.Completed, endedOutcome);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).LongCompleted);
    }

    [Fact]
    public void DefectUnlock_UnlockDuringLongBreak_InStrictMode_DoesNotDiscardBreak()
    {
        _settings.StrictMode = true;
        _engine.UpdateSettings(_settings);

        int breakEndedCount = 0;
        BreakInfo? endedBreak = null;
        BreakOutcome? endedOutcome = null;
        _engine.BreakEnded += (info, outcome) =>
        {
            breakEndedCount++;
            endedBreak = info;
            endedOutcome = outcome;
        };

        Assert.True(_engine.StartLongBreakNow());
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);

        for (int i = 0; i < 15; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Locked);

        for (int i = 0; i < 30; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.NotNull(_engine.CurrentBreak);
        Assert.Equal(TimeSpan.FromSeconds(300 - 45), _engine.CurrentBreak.RemainingDuration);
        Assert.Equal(0, breakEndedCount);

        for (int i = 0; i < 255; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Null(_engine.CurrentBreak);
        Assert.Equal(1, breakEndedCount);
        Assert.NotNull(endedBreak);
        Assert.Equal(BreakKind.Long, endedBreak.Kind);
        Assert.Equal(BreakOutcome.Completed, endedOutcome);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).LongCompleted);
    }

    [Fact]
    public void DefectUnlock_UnlockDuringShortBreak_ContinuesAndCompletesNormally()
    {
        int breakEndedCount = 0;
        BreakInfo? endedBreak = null;
        BreakOutcome? endedOutcome = null;
        _engine.BreakEnded += (info, outcome) =>
        {
            breakEndedCount++;
            endedBreak = info;
            endedOutcome = outcome;
        };

        Assert.True(_engine.StartShortBreakNow());
        Assert.Equal(EnginePhase.ShortBreak, _engine.Phase);
        Assert.NotNull(_engine.CurrentBreak);
        Assert.Equal(TimeSpan.FromSeconds(15), _engine.CurrentBreak.RemainingDuration);

        for (int i = 0; i < 5; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Locked);

        for (int i = 0; i < 3; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        Assert.Equal(EnginePhase.ShortBreak, _engine.Phase);
        Assert.NotNull(_engine.CurrentBreak);
        Assert.Equal(TimeSpan.FromSeconds(7), _engine.CurrentBreak.RemainingDuration);
        Assert.Equal(0, breakEndedCount);

        for (int i = 0; i < 7; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Null(_engine.CurrentBreak);
        Assert.Equal(1, breakEndedCount);
        Assert.NotNull(endedBreak);
        Assert.Equal(BreakKind.Short, endedBreak.Kind);
        Assert.Equal(BreakOutcome.Completed, endedOutcome);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).ShortCompleted);
    }

    [Fact]
    public void DefectUnlock_ResumedDuringLongBreak_BreakContinues()
    {
        int breakEndedCount = 0;
        BreakInfo? endedBreak = null;
        BreakOutcome? endedOutcome = null;
        _engine.BreakEnded += (info, outcome) =>
        {
            breakEndedCount++;
            endedBreak = info;
            endedOutcome = outcome;
        };

        Assert.True(_engine.StartLongBreakNow());
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);

        for (int i = 0; i < 20; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Suspending);

        for (int i = 0; i < 40; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Resumed);

        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);
        Assert.NotNull(_engine.CurrentBreak);
        Assert.Equal(TimeSpan.FromSeconds(300 - 60), _engine.CurrentBreak.RemainingDuration);
        Assert.Equal(0, breakEndedCount);

        for (int i = 0; i < 240; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Null(_engine.CurrentBreak);
        Assert.Equal(1, breakEndedCount);
        Assert.NotNull(endedBreak);
        Assert.Equal(BreakKind.Long, endedBreak.Kind);
        Assert.Equal(BreakOutcome.Completed, endedOutcome);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).LongCompleted);
    }

    [Fact]
    public void DefectUnlock_LockDuringLongBreak_BreakCompletesWhileLocked_ThenUnlock_LandsInWorking()
    {
        int breakEndedCount = 0;
        BreakInfo? endedBreak = null;
        BreakOutcome? endedOutcome = null;
        _engine.BreakEnded += (info, outcome) =>
        {
            breakEndedCount++;
            endedBreak = info;
            endedOutcome = outcome;
        };

        Assert.True(_engine.StartLongBreakNow());
        Assert.Equal(EnginePhase.LongBreak, _engine.Phase);

        for (int i = 0; i < 10; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Locked);

        // Advance remaining 290 seconds while locked -> break completes
        for (int i = 0; i < 290; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Away, _engine.Phase);
        Assert.Null(_engine.CurrentBreak);
        Assert.Equal(1, breakEndedCount);
        Assert.NotNull(endedBreak);
        Assert.Equal(BreakKind.Long, endedBreak.Kind);
        Assert.Equal(BreakOutcome.Completed, endedOutcome);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).LongCompleted);

        // Advance 60 seconds more while still locked
        for (int i = 0; i < 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        // Now unlock session
        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        // Exactly one BreakEnded(Completed) was raised, phase is Working
        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(1, breakEndedCount);
        Assert.Equal(1, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).LongCompleted);

        // Further active ticks accumulate normally in Working
        for (int i = 0; i < 10; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.FromSeconds(10), _engine.SinceLongBreak);
        Assert.Equal(TimeSpan.FromSeconds(10), _engine.SinceShortBreak);
    }

    [Fact]
    public void DefectPause_Pause1Hour_LockedAndUnlocked_StaysPaused_PausedUntilUtcUnchanged()
    {
        _engine.PauseFor(TimeSpan.FromHours(1));
        var expectedPausedUntil = _engine.PausedUntilUtc;
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        for (int i = 0; i < 5; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Locked);

        for (int i = 0; i < 30; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        for (int i = 0; i < 1; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Paused, _engine.Phase);
        Assert.Equal(expectedPausedUntil, _engine.PausedUntilUtc);
    }

    [Fact]
    public void DefectPause_Pause1Hour_SuspendingAndResumed_StaysPaused()
    {
        _engine.PauseFor(TimeSpan.FromHours(1));
        var expectedPausedUntil = _engine.PausedUntilUtc;
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        for (int i = 0; i < 5; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Suspending);

        for (int i = 0; i < 30; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        _engine.OnSessionEvent(SessionEventKind.Resumed);

        for (int i = 0; i < 1; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Paused, _engine.Phase);
        Assert.Equal(expectedPausedUntil, _engine.PausedUntilUtc);
    }

    [Fact]
    public void DefectPause_PauseUntilResumed_LockedAndUnlocked_StaysPaused()
    {
        _engine.PauseFor(null);
        Assert.Equal(EnginePhase.Paused, _engine.Phase);
        Assert.Equal(DateTimeOffset.MaxValue, _engine.PausedUntilUtc);

        for (int i = 0; i < 5; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Locked);

        for (int i = 0; i < 30; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        for (int i = 0; i < 1; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Paused, _engine.Phase);
        Assert.Equal(DateTimeOffset.MaxValue, _engine.PausedUntilUtc);
    }

    [Fact]
    public void DefectPause_PauseExpiresWhileLocked_WithTicks_ThenUnlock_LandsInWorking_CountersReset_NoAbsenceCredit()
    {
        for (int i = 0; i < 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(TimeSpan.FromSeconds(60), _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.FromSeconds(60), _engine.SinceLongBreak);

        BreakKind? absenceCredited = null;
        _engine.AbsenceCredited += kind => absenceCredited = kind;

        _engine.PauseFor(TimeSpan.FromMinutes(10));
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        for (int i = 0; i < 5; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Locked);

        // Advance 15 minutes (900 s) ticking while locked. Pause expires at 10 minutes.
        for (int i = 0; i < 900; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Null(_engine.PausedUntilUtc);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);

        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Null(_engine.PausedUntilUtc);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);
        Assert.Null(absenceCredited);
        Assert.Equal(0, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).AbsencesCredited);

        for (int i = 0; i < 10; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.FromSeconds(10), _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.FromSeconds(10), _engine.SinceLongBreak);
    }

    [Fact]
    public void DefectPause_PauseExpiresWhileLocked_WithoutTicks_ThenUnlock_LandsInWorking_CountersReset_NoAbsenceCredit()
    {
        for (int i = 0; i < 60; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        BreakKind? absenceCredited = null;
        _engine.AbsenceCredited += kind => absenceCredited = kind;

        _engine.PauseFor(TimeSpan.FromMinutes(10));
        Assert.Equal(EnginePhase.Paused, _engine.Phase);

        for (int i = 0; i < 5; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        _engine.OnSessionEvent(SessionEventKind.Locked);

        // Advance 15 minutes with no ticks (e.g. sleep/lock gap)
        _time.Advance(TimeSpan.FromMinutes(15));

        // Unlock session
        _engine.OnSessionEvent(SessionEventKind.Unlocked);

        // First tick after unlock detects pause expiration
        _engine.Tick();

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Null(_engine.PausedUntilUtc);
        Assert.Equal(TimeSpan.Zero, _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, _engine.SinceLongBreak);
        Assert.Null(absenceCredited);
        Assert.Equal(0, _engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).AbsencesCredited);

        for (int i = 0; i < 10; i++)
        {
            _time.Advance(TimeSpan.FromSeconds(1));
            _engine.Tick();
        }

        Assert.Equal(EnginePhase.Working, _engine.Phase);
        Assert.Equal(TimeSpan.FromSeconds(10), _engine.SinceShortBreak);
        Assert.Equal(TimeSpan.FromSeconds(10), _engine.SinceLongBreak);
    }
}
