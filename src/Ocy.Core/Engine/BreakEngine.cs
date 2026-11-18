using Ocy.Abstractions;
using Ocy.Core.Exercises;
using Ocy.Core.Persistence;
using Ocy.Core.Platform;
using Ocy.Core.Settings;
using Ocy.Core.Statistics;

namespace Ocy.Core.Engine;

public sealed class BreakEngine : IDisposable
{
    private readonly IPlatformServices _platform;
    private readonly TimeProvider _timeProvider;
    private readonly IExercisePicker _exercisePicker;
    private readonly AppDataStore? _dataStore;
    private readonly StatisticsData _stats;

    private OcySettings _settings;
    private EnginePhase _phase = EnginePhase.Working;
    private TimeSpan _sinceShortBreak = TimeSpan.Zero;
    private TimeSpan _sinceLongBreak = TimeSpan.Zero;
    private int _postponesUsed;
    private DateTimeOffset? _pausedUntilUtc;
    private DateTimeOffset? _lastTickUtc;
    private DateTimeOffset? _awayStartedUtc;
    private bool _isSessionAway;
    private bool _isFocusAway;
    private TimeSpan _warningCountdownRemaining = TimeSpan.Zero;
    private CurrentBreakInfo? _currentBreak;
    private string? _previousShortExerciseId;
    private string? _previousBodyExerciseId;
    private string? _previousEyesExerciseId;
    private double _activeSecondsAccumulator;
    private TimeSpan? _pendingStartupGap;

    public event Action<EngineSnapshot>? PhaseChanged;
    public event Action<BreakInfo>? BreakStarted;
    public event Action<BreakInfo, BreakOutcome>? BreakEnded;
    public event Action<TimeSpan>? LongBreakWarningStarted;
    public event Action<BreakKind>? AbsenceCredited;

    public EnginePhase Phase => _phase;

    public OcySettings Settings => _settings;

    public StatisticsData Statistics => _stats;

    public TimeSpan SinceShortBreak => _sinceShortBreak;

    public TimeSpan SinceLongBreak => _sinceLongBreak;

    public int PostponesUsed => _postponesUsed;

    public DateTimeOffset? PausedUntilUtc => _pausedUntilUtc;

    public DateTimeOffset? LastTickUtc => _lastTickUtc;

    public CurrentBreakInfo? CurrentBreak => _currentBreak;

    public EngineSnapshot Snapshot => CreateSnapshot();

    public BreakEngine(
        OcySettings? settings = null,
        IPlatformServices? platformServices = null,
        TimeProvider? timeProvider = null,
        AppDataStore? dataStore = null,
        IExercisePicker? exercisePicker = null,
        EngineState? initialState = null,
        StatisticsData? initialStats = null)
    {
        _settings = (settings ?? new OcySettings()).Clone();
        _settings.Clamp();

        _platform = platformServices ?? UnsupportedPlatformServices.Instance;
        _timeProvider = timeProvider ?? TimeProvider.System;
        _exercisePicker = exercisePicker ?? new ExercisePicker();
        _dataStore = dataStore;

        _stats = initialStats ?? (_dataStore != null ? _dataStore.LoadStats() : new StatisticsData());

        var stateToLoad = initialState ?? (_dataStore != null ? _dataStore.LoadState() : null);
        InitializeState(stateToLoad);
    }

    private void InitializeState(EngineState? state)
    {
        var now = _timeProvider.GetUtcNow();
        if (state == null)
        {
            _lastTickUtc = now;
            return;
        }

        _sinceShortBreak = state.SinceShortBreak;
        _sinceLongBreak = state.SinceLongBreak;
        _postponesUsed = state.PostponesUsed;
        _pausedUntilUtc = state.PausedUntilUtc;

        if (_pausedUntilUtc.HasValue)
        {
            if (now >= _pausedUntilUtc.Value)
            {
                EndPause();
            }
            else
            {
                _phase = EnginePhase.Paused;
            }
        }
        else if (state.LastTickUtc.HasValue)
        {
            var gap = now - state.LastTickUtc.Value;
            if (gap > TimeSpan.FromSeconds(15))
            {
                _pendingStartupGap = gap;
            }
        }

        _lastTickUtc = now;
    }

    public void Tick()
    {
        var now = _timeProvider.GetUtcNow();

        // Rule 6: An active pause survives restarts, sleep gaps, and session events.
        if (_phase == EnginePhase.Paused)
        {
            _pendingStartupGap = null;
            _lastTickUtc = now;
            if (_pausedUntilUtc.HasValue && now >= _pausedUntilUtc.Value)
            {
                EndPause();
            }
            return;
        }

        if (_pendingStartupGap.HasValue)
        {
            var gap = _pendingStartupGap.Value;
            _pendingStartupGap = null;
            _lastTickUtc = now;
            ApplyReturnSpan(gap, now);
            return;
        }

        if (_lastTickUtc == null)
        {
            _lastTickUtc = now;
            return;
        }

        var dt = now - _lastTickUtc.Value;
        _lastTickUtc = now;

        if (dt <= TimeSpan.Zero)
        {
            return;
        }

        // Rule 1: Gap = sleep. If dt > 15s, treat whole dt as away span and do not add to counters.
        // Away detection scope is only Working, Deferred and LongBreakWarning.
        if (dt > TimeSpan.FromSeconds(15))
        {
            if (_phase == EnginePhase.ShortBreak || _phase == EnginePhase.LongBreak)
            {
                var focusInBreak = _platform.Focus.IsSupported ? _platform.Focus.GetState() : FocusState.Unknown;
                if (focusInBreak == FocusState.NotPresent)
                {
                    _isFocusAway = true;
                }
                UpdateBreak(dt, now);
                return;
            }

            // Rule 8: A gap while already Away extends the same away span (measured from original awayStartedUtc).
            // Stay Away, keep awayStartedUtc, and credit once when the user returns.
            if (_phase == EnginePhase.Away)
            {
                return;
            }

            if (_phase == EnginePhase.LongBreakWarning)
            {
                _warningCountdownRemaining = TimeSpan.Zero;
            }

            ApplyReturnSpan(dt, now);
            return;
        }

        // Check focus for NotPresent (session lock / screen saver)
        var focus = _platform.Focus.IsSupported ? _platform.Focus.GetState() : FocusState.Unknown;
        if (focus == FocusState.NotPresent)
        {
            if (!_isFocusAway)
            {
                _isFocusAway = true;
                if (_phase == EnginePhase.Working || _phase == EnginePhase.Deferred || _phase == EnginePhase.LongBreakWarning)
                {
                    EnterAway(now);
                }
            }
        }
        else if (_isFocusAway)
        {
            _isFocusAway = false;
            if (!_isSessionAway && _phase != EnginePhase.ShortBreak && _phase != EnginePhase.LongBreak)
            {
                ReturnFromAway(now);
                return;
            }
        }

        // Rule 3: Inside a break
        if (_phase == EnginePhase.ShortBreak || _phase == EnginePhase.LongBreak)
        {
            UpdateBreak(dt, now);
            return;
        }

        if (_isSessionAway || _isFocusAway)
        {
            // Frozen in Away
            return;
        }

        // Rule 4: Away / Inactivity
        if (_phase == EnginePhase.Away)
        {
            var idle = _platform.Idle.IsSupported ? _platform.Idle.GetIdleTime() : TimeSpan.Zero;
            if (idle < _settings.AwayThreshold)
            {
                ReturnFromAway(now);
            }
            return;
        }

        if (_settings.InactivityTracking && _platform.Idle.IsSupported)
        {
            var idle = _platform.Idle.GetIdleTime();
            if (idle >= _settings.AwayThreshold)
            {
                if (_phase == EnginePhase.Working || _phase == EnginePhase.Deferred || _phase == EnginePhase.LongBreakWarning)
                {
                    EnterAway(now - idle);
                    return;
                }
            }
        }

        // LongBreakWarning
        if (_phase == EnginePhase.LongBreakWarning)
        {
            _warningCountdownRemaining -= dt;
            if (_warningCountdownRemaining <= TimeSpan.Zero)
            {
                StartLongBreak();
            }
            return;
        }

        // Deferred
        if (_phase == EnginePhase.Deferred)
        {
            bool isFullscreenNow = _settings.PauseDuringFullscreen && (focus == FocusState.FullscreenApp || focus == FocusState.Presentation);
            if (!isFullscreenNow)
            {
                if (_settings.LongBreaks.WarningSeconds > 0)
                {
                    StartLongBreakWarning();
                }
                else
                {
                    StartLongBreak();
                }
            }
            return;
        }

        // Rule 5: Working
        if (_phase == EnginePhase.Working)
        {
            _sinceShortBreak += dt;
            _sinceLongBreak += dt;
            _activeSecondsAccumulator += dt.TotalSeconds;
            while (_activeSecondsAccumulator >= 60.0)
            {
                _activeSecondsAccumulator -= 60.0;
                _stats.GetOrCreate(GetLocalDate()).ActiveMinutes++;
            }

            bool isFullscreen = _settings.PauseDuringFullscreen && (focus == FocusState.FullscreenApp || focus == FocusState.Presentation);

            // Check long break warning due
            if (_settings.LongBreaks.Enabled)
            {
                var warningAt = _settings.LongBreaks.Interval - _settings.LongBreaks.WarningDuration;
                if (_sinceLongBreak >= warningAt)
                {
                    if (isFullscreen)
                    {
                        TransitionTo(EnginePhase.Deferred);
                        return;
                    }

                    if (_settings.LongBreaks.WarningSeconds > 0)
                    {
                        StartLongBreakWarning();
                        return;
                    }

                    if (_sinceLongBreak >= _settings.LongBreaks.Interval)
                    {
                        StartLongBreak();
                        return;
                    }
                }
            }

            // Check short break due
            if (_settings.ShortBreaks.Enabled && _sinceShortBreak >= _settings.ShortBreaks.Interval)
            {
                if (isFullscreen)
                {
                    // Short break dropped during fullscreen
                    _sinceShortBreak = TimeSpan.Zero;
                    return;
                }

                if (ScheduleCalculator.IsShortBreakSuppressedByLongBreak(_settings, _sinceLongBreak))
                {
                    // Reset short counter silently when long break is due within 2 min
                    _sinceShortBreak = TimeSpan.Zero;
                    return;
                }

                StartShortBreak();
            }
        }
    }

    public bool StartLongBreakNow()
    {
        if (_phase == EnginePhase.Working || _phase == EnginePhase.LongBreakWarning || _phase == EnginePhase.Deferred)
        {
            StartLongBreak();
            return true;
        }

        return false;
    }

    public bool StartShortBreakNow()
    {
        if (_phase == EnginePhase.Working || _phase == EnginePhase.Deferred)
        {
            StartShortBreak();
            return true;
        }

        return false;
    }

    public bool PostponeLongBreak()
    {
        if (_phase != EnginePhase.LongBreakWarning)
        {
            return false;
        }

        if (_settings.StrictMode)
        {
            return false;
        }

        if (_postponesUsed >= _settings.LongBreaks.MaxPostpones)
        {
            return false;
        }

        _postponesUsed++;
        _stats.GetOrCreate(GetLocalDate()).LongPostponed++;

        // Long break is due again after postponeMinutes
        _sinceLongBreak = _settings.LongBreaks.Interval - _settings.LongBreaks.PostponeDuration;
        TransitionTo(EnginePhase.Working);
        return true;
    }

    public bool SkipCurrentBreak()
    {
        var today = _stats.GetOrCreate(GetLocalDate());

        if (_phase == EnginePhase.ShortBreak)
        {
            var breakInfo = _currentBreak != null
                ? new BreakInfo(_currentBreak.Kind, _currentBreak.ExerciseId, _currentBreak.TotalDuration, _currentBreak.SuggestionExerciseIds)
                : new BreakInfo(BreakKind.Short, string.Empty, _settings.ShortBreaks.Duration);

            _currentBreak = null;
            _sinceShortBreak = TimeSpan.Zero;
            today.ShortSkipped++;
            BreakEnded?.Invoke(breakInfo, BreakOutcome.Skipped);

            if (_isSessionAway || _isFocusAway)
            {
                EnterAway(_timeProvider.GetUtcNow());
            }
            else
            {
                TransitionTo(EnginePhase.Working);
            }
            return true;
        }

        if (_phase == EnginePhase.LongBreak)
        {
            if (_settings.StrictMode)
            {
                return false;
            }

            var breakInfo = _currentBreak != null
                ? new BreakInfo(_currentBreak.Kind, _currentBreak.ExerciseId, _currentBreak.TotalDuration, _currentBreak.SuggestionExerciseIds)
                : new BreakInfo(BreakKind.Long, string.Empty, _settings.LongBreaks.Duration);

            _currentBreak = null;
            _sinceLongBreak = TimeSpan.Zero;
            _sinceShortBreak = TimeSpan.Zero;
            _postponesUsed = 0;
            today.LongSkipped++;
            BreakEnded?.Invoke(breakInfo, BreakOutcome.Skipped);

            if (_isSessionAway || _isFocusAway)
            {
                EnterAway(_timeProvider.GetUtcNow());
            }
            else
            {
                TransitionTo(EnginePhase.Working);
            }
            return true;
        }

        if (_phase == EnginePhase.LongBreakWarning)
        {
            if (_settings.StrictMode)
            {
                return false;
            }

            _warningCountdownRemaining = TimeSpan.Zero;
            _sinceLongBreak = TimeSpan.Zero;
            _sinceShortBreak = TimeSpan.Zero;
            _postponesUsed = 0;
            today.LongSkipped++;
            TransitionTo(EnginePhase.Working);
            return true;
        }

        return false;
    }

    public void PauseFor(TimeSpan? duration)
    {
        var now = _timeProvider.GetUtcNow();
        _pausedUntilUtc = duration.HasValue ? now + duration.Value : DateTimeOffset.MaxValue;

        if (_phase == EnginePhase.ShortBreak || _phase == EnginePhase.LongBreak)
        {
            var breakInfo = _currentBreak != null
                ? new BreakInfo(_currentBreak.Kind, _currentBreak.ExerciseId, _currentBreak.TotalDuration, _currentBreak.SuggestionExerciseIds)
                : new BreakInfo(_phase == EnginePhase.ShortBreak ? BreakKind.Short : BreakKind.Long, string.Empty, TimeSpan.Zero);

            _currentBreak = null;
            BreakEnded?.Invoke(breakInfo, BreakOutcome.Interrupted);
        }
        else
        {
            _currentBreak = null;
        }

        TransitionTo(EnginePhase.Paused);
    }

    public void Resume()
    {
        if (_phase == EnginePhase.Paused)
        {
            EndPause();
        }
    }

    public void OnSessionEvent(SessionEventKind kind)
    {
        var now = _timeProvider.GetUtcNow();
        switch (kind)
        {
            case SessionEventKind.Locked:
            case SessionEventKind.Suspending:
                if (!_isSessionAway)
                {
                    _isSessionAway = true;
                    if (_phase == EnginePhase.Working || _phase == EnginePhase.Deferred || _phase == EnginePhase.LongBreakWarning)
                    {
                        EnterAway(now);
                    }
                }
                break;

            case SessionEventKind.Unlocked:
            case SessionEventKind.Resumed:
                if (_isSessionAway)
                {
                    _isSessionAway = false;
                    if (_phase == EnginePhase.ShortBreak || _phase == EnginePhase.LongBreak)
                    {
                        break;
                    }

                    if (!_isFocusAway)
                    {
                        ReturnFromAway(now);
                    }
                }
                break;
        }
    }

    public void UpdateSettings(OcySettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        _settings = settings.Clone();
        _settings.Clamp();
        PhaseChanged?.Invoke(CreateSnapshot());
    }

    public EngineState ExportState() => new()
    {
        SchemaVersion = 1,
        SinceShortBreak = _sinceShortBreak,
        SinceLongBreak = _sinceLongBreak,
        PostponesUsed = _postponesUsed,
        PausedUntilUtc = _pausedUntilUtc,
        LastTickUtc = _lastTickUtc,
    };

    public void Save()
    {
        if (_dataStore == null)
        {
            return;
        }

        _dataStore.SaveState(ExportState());
        _stats.PruneRetention(GetLocalDate());
        _dataStore.SaveStats(_stats);
    }

    private void StartShortBreak()
    {
        var exercise = _exercisePicker.PickShortExercise(_settings.HasWindowNearby, _previousShortExerciseId);
        _previousShortExerciseId = exercise.Id;
        var duration = _settings.ShortBreaks.Duration;

        _currentBreak = new CurrentBreakInfo(BreakKind.Short, exercise.Id, duration, duration);
        TransitionTo(EnginePhase.ShortBreak);

        var breakInfo = new BreakInfo(BreakKind.Short, exercise.Id, duration);
        BreakStarted?.Invoke(breakInfo);
    }

    private void StartLongBreakWarning()
    {
        _warningCountdownRemaining = _settings.LongBreaks.WarningDuration;
        TransitionTo(EnginePhase.LongBreakWarning);
        LongBreakWarningStarted?.Invoke(_warningCountdownRemaining);
    }

    private void StartLongBreak()
    {
        var (bodyEx, eyesEx) = _exercisePicker.PickLongExercises(
            _settings.HasWindowNearby,
            _previousEyesExerciseId,
            _previousBodyExerciseId);

        _previousEyesExerciseId = eyesEx.Id;
        _previousBodyExerciseId = bodyEx.Id;

        var suggestions = new List<string> { bodyEx.Id, eyesEx.Id };
        var duration = _settings.LongBreaks.Duration;

        _currentBreak = new CurrentBreakInfo(BreakKind.Long, bodyEx.Id, duration, duration, suggestions);
        TransitionTo(EnginePhase.LongBreak);

        var breakInfo = new BreakInfo(BreakKind.Long, bodyEx.Id, duration, suggestions);
        BreakStarted?.Invoke(breakInfo);
    }

    private void UpdateBreak(TimeSpan dt, DateTimeOffset now)
    {
        if (_currentBreak == null)
        {
            if (_isSessionAway || _isFocusAway)
            {
                EnterAway(now);
            }
            else
            {
                TransitionTo(EnginePhase.Working);
            }
            return;
        }

        var remaining = _currentBreak.RemainingDuration - dt;
        if (remaining <= TimeSpan.Zero)
        {
            var breakEndedUtc = now - (dt - _currentBreak.RemainingDuration);
            var breakInfo = new BreakInfo(
                _currentBreak.Kind,
                _currentBreak.ExerciseId,
                _currentBreak.TotalDuration,
                _currentBreak.SuggestionExerciseIds);

            var today = _stats.GetOrCreate(GetLocalDate());

            if (_currentBreak.Kind == BreakKind.Short)
            {
                _sinceShortBreak = TimeSpan.Zero;
                today.ShortCompleted++;
            }
            else
            {
                _sinceLongBreak = TimeSpan.Zero;
                _sinceShortBreak = TimeSpan.Zero;
                _postponesUsed = 0;
                today.LongCompleted++;
            }

            _currentBreak = null;
            BreakEnded?.Invoke(breakInfo, BreakOutcome.Completed);

            if (_isSessionAway || _isFocusAway)
            {
                EnterAway(breakEndedUtc);
            }
            else
            {
                TransitionTo(EnginePhase.Working);
            }
        }
        else
        {
            _currentBreak = new CurrentBreakInfo(
                _currentBreak.Kind,
                _currentBreak.ExerciseId,
                _currentBreak.TotalDuration,
                remaining,
                _currentBreak.SuggestionExerciseIds);
        }
    }

    private void EnterAway(DateTimeOffset awayStartedUtc)
    {
        if (_phase == EnginePhase.Away)
        {
            return;
        }

        _awayStartedUtc = awayStartedUtc;
        _warningCountdownRemaining = TimeSpan.Zero;
        TransitionTo(EnginePhase.Away);
    }

    private void ReturnFromAway(DateTimeOffset now)
    {
        var away = now - (_awayStartedUtc ?? now);
        _awayStartedUtc = null;
        _lastTickUtc = now;

        if (_phase != EnginePhase.Away)
        {
            return;
        }

        ApplyReturnRules(away);
        TransitionTo(EnginePhase.Working);
    }

    private void ApplyReturnSpan(TimeSpan away, DateTimeOffset now)
    {
        ApplyReturnRules(away);

        // Check if user is currently idle
        if (_settings.InactivityTracking && _platform.Idle.IsSupported)
        {
            var idle = _platform.Idle.GetIdleTime();
            if (idle >= _settings.AwayThreshold)
            {
                EnterAway(now - idle);
                return;
            }
        }

        var focus = _platform.Focus.IsSupported ? _platform.Focus.GetState() : FocusState.Unknown;
        if (focus == FocusState.NotPresent)
        {
            _isFocusAway = true;
            EnterAway(now);
            return;
        }

        TransitionTo(EnginePhase.Working);
    }

    private void ApplyReturnRules(TimeSpan away)
    {
        if (away >= _settings.LongBreaks.Duration)
        {
            _sinceShortBreak = TimeSpan.Zero;
            _sinceLongBreak = TimeSpan.Zero;
            _postponesUsed = 0;
            _stats.GetOrCreate(GetLocalDate()).AbsencesCredited++;
            AbsenceCredited?.Invoke(BreakKind.Long);
        }
        else if (away >= _settings.ShortBreaks.Duration)
        {
            _sinceShortBreak = TimeSpan.Zero;
        }
    }

    private void EndPause()
    {
        _pausedUntilUtc = null;
        _sinceShortBreak = TimeSpan.Zero;
        _sinceLongBreak = TimeSpan.Zero;
        _postponesUsed = 0;
        TransitionTo(EnginePhase.Working);
    }

    private void TransitionTo(EnginePhase newPhase)
    {
        if (_phase == newPhase)
        {
            return;
        }

        _phase = newPhase;
        PhaseChanged?.Invoke(CreateSnapshot());
    }

    public EngineSnapshot CreateSnapshot()
    {
        var timeUntilShort = ScheduleCalculator.GetTimeUntilShortBreak(_settings, _sinceShortBreak);
        var timeUntilLong = ScheduleCalculator.GetTimeUntilLongBreak(_settings, _sinceLongBreak);
        var postponesLeft = Math.Max(0, _settings.LongBreaks.MaxPostpones - _postponesUsed);

        return new EngineSnapshot(
            Phase: _phase,
            TimeUntilShortBreak: timeUntilShort,
            TimeUntilLongBreak: timeUntilLong,
            CurrentBreak: _currentBreak,
            PausedUntilUtc: _pausedUntilUtc,
            PostponesLeft: postponesLeft,
            StrictMode: _settings.StrictMode);
    }

    private DateOnly GetLocalDate()
    {
        var localNow = TimeZoneInfo.ConvertTime(_timeProvider.GetUtcNow(), _timeProvider.LocalTimeZone);
        return DateOnly.FromDateTime(localNow.DateTime);
    }

    public void Dispose()
    {
        Save();
    }
}
