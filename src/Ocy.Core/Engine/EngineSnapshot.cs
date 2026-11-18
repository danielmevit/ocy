namespace Ocy.Core.Engine;

public sealed record EngineSnapshot(
    EnginePhase Phase,
    TimeSpan? TimeUntilShortBreak,
    TimeSpan? TimeUntilLongBreak,
    CurrentBreakInfo? CurrentBreak,
    DateTimeOffset? PausedUntilUtc,
    int PostponesLeft,
    bool StrictMode);
