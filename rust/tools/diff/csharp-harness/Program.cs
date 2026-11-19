using System.Globalization;
using System.Text;
using System.Text.Json;
using Microsoft.Extensions.Time.Testing;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Exercises;
using Ocy.Core.Persistence;
using Ocy.Core.Settings;
using Ocy.Core.Statistics;

namespace Ocy.DiffHarness;

internal static class Program
{
    private sealed class FakeIdleProvider : IUserIdleProvider
    {
        public bool IsSupported { get; set; } = true;
        public TimeSpan IdleTime { get; set; } = TimeSpan.Zero;
        public TimeSpan GetIdleTime() => IsSupported ? IdleTime : TimeSpan.Zero;
    }

    private sealed class FakeFocusStateProvider : IFocusStateProvider
    {
        public bool IsSupported { get; set; } = true;
        public FocusState State { get; set; } = FocusState.Normal;
        public FocusState GetState() => IsSupported ? State : FocusState.Unknown;
    }

    private sealed class FakeAutostartService : IAutostartService
    {
        public bool IsSupported => false;
        public bool IsEnabled() => false;
        public void SetEnabled(bool enabled) { }
    }

    private sealed class FakeSessionEventSource : ISessionEventSource
    {
        public bool IsSupported => true;
        public event EventHandler<SessionEventKind>? SessionChanged
        {
            add { }
            remove { }
        }
        public void Start() { }
        public void Dispose() { }
    }

    private sealed class FakePlatformServices : IPlatformServices
    {
        public FakeIdleProvider FakeIdle { get; } = new();
        public FakeFocusStateProvider FakeFocus { get; } = new();
        public IUserIdleProvider Idle => FakeIdle;
        public IFocusStateProvider Focus => FakeFocus;
        public IAutostartService Autostart { get; } = new FakeAutostartService();
        public ISessionEventSource Session { get; } = new FakeSessionEventSource();
        public void Dispose() { }
    }

    private sealed class FirstCandidateRandom : Random
    {
        public override int Next() => 0;
        public override int Next(int maxValue) => 0;
        public override int Next(int minValue, int maxValue) => minValue;
    }

    private sealed record RecordedEvent(
        string Type,
        EnginePhase? Phase = null,
        BreakKind? Kind = null,
        BreakOutcome? Outcome = null,
        string? ExerciseId = null,
        long? TotalMs = null,
        IReadOnlyList<string>? SuggestionExerciseIds = null,
        long? DurationMs = null);

    public static int Main(string[] args)
    {
        if (args.Length != 1)
        {
            Console.Error.WriteLine("Usage: CsharpHarness <scenario.json>");
            return 2;
        }

        string jsonText = File.ReadAllText(args[0], Encoding.UTF8);
        using var doc = JsonDocument.Parse(jsonText);
        JsonElement root = doc.RootElement;

        var initialUtc = DateTimeOffset.Parse(
            root.GetProperty("initialUtc").GetString()!,
            CultureInfo.InvariantCulture,
            DateTimeStyles.RoundtripKind);
        string tzId = root.GetProperty("timeZone").GetString()!;
        var timeZone = TimeZoneInfo.FindSystemTimeZoneById(tzId);

        var timeProvider = new FakeTimeProvider(initialUtc);
        timeProvider.SetLocalTimeZone(timeZone);

        var settings = root.TryGetProperty("settings", out JsonElement settingsEl) && settingsEl.ValueKind != JsonValueKind.Null
            ? JsonSerializer.Deserialize(settingsEl.GetRawText(), OcyJsonContext.Default.OcySettings) ?? new OcySettings()
            : new OcySettings();

        EngineState? initialState = null;
        if (root.TryGetProperty("initialState", out JsonElement stateEl) && stateEl.ValueKind != JsonValueKind.Null)
        {
            initialState = JsonSerializer.Deserialize(stateEl.GetRawText(), OcyJsonContext.Default.EngineState);
        }

        using var platform = new FakePlatformServices();
        if (root.TryGetProperty("idleSupported", out JsonElement idleSupEl) && idleSupEl.ValueKind != JsonValueKind.Null)
        {
            platform.FakeIdle.IsSupported = idleSupEl.GetBoolean();
        }
        if (root.TryGetProperty("focusSupported", out JsonElement focusSupEl) && focusSupEl.ValueKind != JsonValueKind.Null)
        {
            platform.FakeFocus.IsSupported = focusSupEl.GetBoolean();
        }

        var picker = new ExercisePicker(new FirstCandidateRandom());
        using var engine = new BreakEngine(
            settings: settings,
            platformServices: platform,
            timeProvider: timeProvider,
            dataStore: null,
            exercisePicker: picker,
            initialState: initialState);

        var events = new List<RecordedEvent>();
        engine.PhaseChanged += snap => events.Add(new RecordedEvent("PhaseChanged", Phase: snap.Phase));
        engine.BreakStarted += info => events.Add(new RecordedEvent(
            "BreakStarted",
            Kind: info.Kind,
            ExerciseId: info.ExerciseId,
            TotalMs: ToMs(info.TotalDuration),
            SuggestionExerciseIds: info.SuggestionExerciseIds));
        engine.BreakEnded += (info, outcome) => events.Add(new RecordedEvent(
            "BreakEnded",
            Kind: info.Kind,
            Outcome: outcome,
            ExerciseId: info.ExerciseId,
            TotalMs: ToMs(info.TotalDuration),
            SuggestionExerciseIds: info.SuggestionExerciseIds));
        engine.LongBreakWarningStarted += cd => events.Add(new RecordedEvent(
            "LongBreakWarningStarted",
            DurationMs: ToMs(cd)));
        engine.AbsenceCredited += kind => events.Add(new RecordedEvent(
            "AbsenceCredited",
            Kind: kind));

        JsonElement stepsEl = root.GetProperty("steps");
        int stepIndex = 0;
        foreach (JsonElement step in stepsEl.EnumerateArray())
        {
            events.Clear();

            long advanceMs = step.GetProperty("advanceMs").GetInt64();
            if (advanceMs > 0)
            {
                timeProvider.Advance(TimeSpan.FromMilliseconds(advanceMs));
            }

            long idleMs = step.GetProperty("idleMs").GetInt64();
            platform.FakeIdle.IdleTime = TimeSpan.FromMilliseconds(idleMs);

            string focusStr = step.GetProperty("focus").GetString()!;
            platform.FakeFocus.State = Enum.Parse<FocusState>(focusStr, ignoreCase: false);

            bool? actionResult = null;
            if (step.TryGetProperty("action", out JsonElement actionEl) && actionEl.ValueKind == JsonValueKind.Object)
            {
                string actionType = actionEl.GetProperty("type").GetString()!;
                switch (actionType)
                {
                    case "none":
                        break;
                    case "tick":
                        engine.Tick();
                        break;
                    case "start_long_now":
                        actionResult = engine.StartLongBreakNow();
                        break;
                    case "start_short_now":
                        actionResult = engine.StartShortBreakNow();
                        break;
                    case "postpone":
                        actionResult = engine.PostponeLongBreak();
                        break;
                    case "skip":
                        actionResult = engine.SkipCurrentBreak();
                        break;
                    case "pause":
                        if (actionEl.TryGetProperty("minutes", out JsonElement minEl) && minEl.ValueKind == JsonValueKind.Number)
                        {
                            engine.PauseFor(TimeSpan.FromMinutes(minEl.GetInt64()));
                        }
                        else
                        {
                            engine.PauseFor(null);
                        }
                        break;
                    case "resume":
                        engine.Resume();
                        break;
                    case "session":
                        string sessionKindStr = actionEl.GetProperty("kind").GetString()!;
                        var sessionKind = Enum.Parse<SessionEventKind>(sessionKindStr, ignoreCase: false);
                        engine.OnSessionEvent(sessionKind);
                        break;
                    case "update_settings":
                        JsonElement newSettingsEl = actionEl.GetProperty("settings");
                        var newSettings = JsonSerializer.Deserialize(newSettingsEl.GetRawText(), OcyJsonContext.Default.OcySettings)
                            ?? new OcySettings();
                        engine.UpdateSettings(newSettings);
                        break;
                    default:
                        throw new InvalidOperationException($"Unknown action type: {actionType}");
                }
            }

            DateOnly localDate = GetLocalDate(timeProvider);
            DailyStatistics todayStats = engine.Statistics.Days.FirstOrDefault(d => d.Date == localDate)
                ?? new DailyStatistics { Date = localDate };

            Console.WriteLine(FormatStepLine(stepIndex, engine, actionResult, events, todayStats));
            stepIndex++;
        }

        EngineState exported = engine.ExportState();
        string exportedJson = JsonSerializer.Serialize(exported, OcyJsonContext.Default.EngineState);
        using var finalStream = new MemoryStream();
        using (var writer = new Utf8JsonWriter(finalStream))
        {
            writer.WriteStartObject();
            writer.WritePropertyName("exportedState");
            using var exportedDoc = JsonDocument.Parse(exportedJson);
            exportedDoc.RootElement.WriteTo(writer);
            writer.WriteEndObject();
        }
        Console.WriteLine(Encoding.UTF8.GetString(finalStream.ToArray()));

        return 0;
    }

    private static DateOnly GetLocalDate(TimeProvider timeProvider)
    {
        DateTimeOffset localNow = TimeZoneInfo.ConvertTime(timeProvider.GetUtcNow(), timeProvider.LocalTimeZone);
        return DateOnly.FromDateTime(localNow.DateTime);
    }

    private static long ToMs(TimeSpan ts) => ts.Ticks / TimeSpan.TicksPerMillisecond;

    private static string? FormatPaused(DateTimeOffset? pausedUntilUtc)
    {
        if (!pausedUntilUtc.HasValue)
        {
            return null;
        }
        if (pausedUntilUtc.Value == DateTimeOffset.MaxValue)
        {
            return "until_resumed";
        }
        DateTimeOffset utc = pausedUntilUtc.Value.ToUniversalTime();
        return utc.ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'", CultureInfo.InvariantCulture);
    }

    private static void WriteCurrentBreak(Utf8JsonWriter writer, CurrentBreakInfo? cb)
    {
        if (cb == null)
        {
            writer.WriteNullValue();
            return;
        }

        writer.WriteStartObject();
        writer.WriteString("kind", cb.Kind.ToString());
        writer.WriteString("exerciseId", cb.ExerciseId);
        writer.WriteNumber("totalMs", ToMs(cb.TotalDuration));
        writer.WriteNumber("remainingMs", ToMs(cb.RemainingDuration));
        writer.WritePropertyName("suggestionExerciseIds");
        if (cb.SuggestionExerciseIds == null)
        {
            writer.WriteNullValue();
        }
        else
        {
            writer.WriteStartArray();
            foreach (string id in cb.SuggestionExerciseIds)
            {
                writer.WriteStringValue(id);
            }
            writer.WriteEndArray();
        }
        writer.WriteEndObject();
    }

    private static void WriteNullableString(Utf8JsonWriter writer, string? value)
    {
        if (value == null)
        {
            writer.WriteNullValue();
        }
        else
        {
            writer.WriteStringValue(value);
        }
    }

    private static string FormatStepLine(
        int stepIndex,
        BreakEngine engine,
        bool? actionResult,
        List<RecordedEvent> events,
        DailyStatistics todayStats)
    {
        EngineSnapshot snap = engine.Snapshot;
        using var stream = new MemoryStream();
        using (var writer = new Utf8JsonWriter(stream))
        {
            writer.WriteStartObject();
            writer.WriteNumber("step", stepIndex);
            writer.WriteString("phase", engine.Phase.ToString());
            writer.WriteNumber("sinceShortMs", ToMs(engine.SinceShortBreak));
            writer.WriteNumber("sinceLongMs", ToMs(engine.SinceLongBreak));
            writer.WriteNumber("postponesUsed", engine.PostponesUsed);

            writer.WritePropertyName("pausedUntil");
            WriteNullableString(writer, FormatPaused(engine.PausedUntilUtc));

            writer.WritePropertyName("currentBreak");
            WriteCurrentBreak(writer, engine.CurrentBreak);

            writer.WritePropertyName("actionResult");
            if (actionResult.HasValue)
            {
                writer.WriteBooleanValue(actionResult.Value);
            }
            else
            {
                writer.WriteNullValue();
            }

            writer.WritePropertyName("events");
            writer.WriteStartArray();
            foreach (RecordedEvent ev in events)
            {
                writer.WriteStartObject();
                writer.WriteString("type", ev.Type);
                if (ev.Phase.HasValue)
                {
                    writer.WriteString("phase", ev.Phase.Value.ToString());
                }
                if (ev.Kind.HasValue)
                {
                    writer.WriteString("kind", ev.Kind.Value.ToString());
                }
                if (ev.Outcome.HasValue)
                {
                    writer.WriteString("outcome", ev.Outcome.Value.ToString());
                }
                if (ev.ExerciseId != null)
                {
                    writer.WriteString("exerciseId", ev.ExerciseId);
                }
                if (ev.TotalMs.HasValue)
                {
                    writer.WriteNumber("totalMs", ev.TotalMs.Value);
                }
                if (ev.SuggestionExerciseIds != null)
                {
                    writer.WritePropertyName("suggestionExerciseIds");
                    writer.WriteStartArray();
                    foreach (string id in ev.SuggestionExerciseIds)
                    {
                        writer.WriteStringValue(id);
                    }
                    writer.WriteEndArray();
                }
                if (ev.DurationMs.HasValue)
                {
                    writer.WriteNumber("durationMs", ev.DurationMs.Value);
                }
                writer.WriteEndObject();
            }
            writer.WriteEndArray();

            writer.WritePropertyName("todayStats");
            writer.WriteStartObject();
            writer.WriteString("date", todayStats.Date.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture));
            writer.WriteNumber("shortCompleted", todayStats.ShortCompleted);
            writer.WriteNumber("shortSkipped", todayStats.ShortSkipped);
            writer.WriteNumber("longCompleted", todayStats.LongCompleted);
            writer.WriteNumber("longSkipped", todayStats.LongSkipped);
            writer.WriteNumber("longPostponed", todayStats.LongPostponed);
            writer.WriteNumber("absencesCredited", todayStats.AbsencesCredited);
            writer.WriteNumber("activeMinutes", todayStats.ActiveMinutes);
            writer.WriteEndObject();

            writer.WritePropertyName("snapshot");
            writer.WriteStartObject();
            writer.WriteString("phase", snap.Phase.ToString());
            writer.WritePropertyName("timeUntilShortBreakMs");
            if (snap.TimeUntilShortBreak.HasValue)
            {
                writer.WriteNumberValue(ToMs(snap.TimeUntilShortBreak.Value));
            }
            else
            {
                writer.WriteNullValue();
            }
            writer.WritePropertyName("timeUntilLongBreakMs");
            if (snap.TimeUntilLongBreak.HasValue)
            {
                writer.WriteNumberValue(ToMs(snap.TimeUntilLongBreak.Value));
            }
            else
            {
                writer.WriteNullValue();
            }
            writer.WritePropertyName("currentBreak");
            WriteCurrentBreak(writer, snap.CurrentBreak);
            writer.WritePropertyName("pausedUntil");
            WriteNullableString(writer, FormatPaused(snap.PausedUntilUtc));
            writer.WriteNumber("postponesLeft", snap.PostponesLeft);
            writer.WriteBoolean("strictMode", snap.StrictMode);
            writer.WriteEndObject();

            writer.WriteEndObject();
        }
        return Encoding.UTF8.GetString(stream.ToArray());
    }
}
