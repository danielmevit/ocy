using System.Text.Json.Serialization;

namespace Ocy.Core.Settings;

public sealed class ShortBreakSettings
{
    public const int MinIntervalMinutes = 2;
    public const int MaxIntervalMinutes = 60;
    public const int MinDurationSeconds = 5;
    public const int MaxDurationSeconds = 120;

    public bool Enabled { get; set; } = true;

    public int IntervalMinutes { get; set; } = 10;

    public int DurationSeconds { get; set; } = 15;

    public bool Fullscreen { get; set; }

    [JsonIgnore]
    public TimeSpan Interval => TimeSpan.FromMinutes(IntervalMinutes);

    [JsonIgnore]
    public TimeSpan Duration => TimeSpan.FromSeconds(DurationSeconds);

    public void Clamp()
    {
        IntervalMinutes = Math.Clamp(IntervalMinutes, MinIntervalMinutes, MaxIntervalMinutes);
        DurationSeconds = Math.Clamp(DurationSeconds, MinDurationSeconds, MaxDurationSeconds);
    }

    public ShortBreakSettings Clone() => new()
    {
        Enabled = Enabled,
        IntervalMinutes = IntervalMinutes,
        DurationSeconds = DurationSeconds,
        Fullscreen = Fullscreen,
    };
}
