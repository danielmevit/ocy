namespace Ocy.Core.Engine;

public sealed class EngineState
{
    public int SchemaVersion { get; set; } = 1;

    public TimeSpan SinceShortBreak { get; set; } = TimeSpan.Zero;

    public TimeSpan SinceLongBreak { get; set; } = TimeSpan.Zero;

    public int PostponesUsed { get; set; }

    public DateTimeOffset? PausedUntilUtc { get; set; }

    public DateTimeOffset? LastTickUtc { get; set; }

    public EngineState Clone() => new()
    {
        SchemaVersion = SchemaVersion,
        SinceShortBreak = SinceShortBreak,
        SinceLongBreak = SinceLongBreak,
        PostponesUsed = PostponesUsed,
        PausedUntilUtc = PausedUntilUtc,
        LastTickUtc = LastTickUtc,
    };
}
