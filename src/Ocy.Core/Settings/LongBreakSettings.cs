using System.Text.Json.Serialization;

namespace Ocy.Core.Settings;

public sealed class LongBreakSettings
{
    public const int MinIntervalMinutes = 15;
    public const int MaxIntervalMinutes = 240;
    public const int MinDurationMinutes = 1;
    public const int MaxDurationMinutes = 30;
    public const int MinWarningSeconds = 0;
    public const int MaxWarningSeconds = 300;
    public const int MinPostponeMinutes = 1;
    public const int MaxPostponeMinutes = 30;
    public const int MinMaxPostpones = 0;
    public const int MaxMaxPostpones = 10;

    public bool Enabled { get; set; } = true;

    public int IntervalMinutes { get; set; } = 60;

    public int DurationMinutes { get; set; } = 5;

    public int WarningSeconds { get; set; } = 60;

    public int PostponeMinutes { get; set; } = 3;

    public int MaxPostpones { get; set; } = 3;

    [JsonIgnore]
    public TimeSpan Interval => TimeSpan.FromMinutes(IntervalMinutes);

    [JsonIgnore]
    public TimeSpan Duration => TimeSpan.FromMinutes(DurationMinutes);

    [JsonIgnore]
    public TimeSpan WarningDuration => TimeSpan.FromSeconds(WarningSeconds);

    [JsonIgnore]
    public TimeSpan PostponeDuration => TimeSpan.FromMinutes(PostponeMinutes);

    public void Clamp()
    {
        IntervalMinutes = Math.Clamp(IntervalMinutes, MinIntervalMinutes, MaxIntervalMinutes);
        DurationMinutes = Math.Clamp(DurationMinutes, MinDurationMinutes, MaxDurationMinutes);
        WarningSeconds = Math.Clamp(WarningSeconds, MinWarningSeconds, MaxWarningSeconds);
        PostponeMinutes = Math.Clamp(PostponeMinutes, MinPostponeMinutes, MaxPostponeMinutes);
        MaxPostpones = Math.Clamp(MaxPostpones, MinMaxPostpones, MaxMaxPostpones);
    }

    public LongBreakSettings Clone() => new()
    {
        Enabled = Enabled,
        IntervalMinutes = IntervalMinutes,
        DurationMinutes = DurationMinutes,
        WarningSeconds = WarningSeconds,
        PostponeMinutes = PostponeMinutes,
        MaxPostpones = MaxPostpones,
    };
}
