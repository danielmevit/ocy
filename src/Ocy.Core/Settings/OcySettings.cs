using System.Text.Json.Serialization;

namespace Ocy.Core.Settings;

public sealed class OcySettings
{
    public const int MinAwayThresholdSeconds = 30;
    public const int MaxAwayThresholdSeconds = 1800;

    public int SchemaVersion { get; set; } = 1;

    public ShortBreakSettings ShortBreaks { get; set; } = new();

    public LongBreakSettings LongBreaks { get; set; } = new();

    public bool StrictMode { get; set; }

    public bool Sounds { get; set; }

    public bool InactivityTracking { get; set; } = true;

    public int AwayThresholdSeconds { get; set; } = 120;

    public bool PauseDuringFullscreen { get; set; } = true;

    public bool HasWindowNearby { get; set; } = true;

    public bool StatusNotifications { get; set; } = true;

    public bool StartWithSystem { get; set; }

    public string Language { get; set; } = "en";

    [JsonIgnore]
    public TimeSpan AwayThreshold => TimeSpan.FromSeconds(AwayThresholdSeconds);

    public void Clamp()
    {
        ShortBreaks ??= new ShortBreakSettings();
        ShortBreaks.Clamp();

        LongBreaks ??= new LongBreakSettings();
        LongBreaks.Clamp();

        AwayThresholdSeconds = Math.Clamp(AwayThresholdSeconds, MinAwayThresholdSeconds, MaxAwayThresholdSeconds);

        if (string.IsNullOrWhiteSpace(Language))
        {
            Language = "en";
        }
    }

    public OcySettings Clone()
    {
        var copy = new OcySettings
        {
            SchemaVersion = SchemaVersion,
            ShortBreaks = ShortBreaks.Clone(),
            LongBreaks = LongBreaks.Clone(),
            StrictMode = StrictMode,
            Sounds = Sounds,
            InactivityTracking = InactivityTracking,
            AwayThresholdSeconds = AwayThresholdSeconds,
            PauseDuringFullscreen = PauseDuringFullscreen,
            HasWindowNearby = HasWindowNearby,
            StatusNotifications = StatusNotifications,
            StartWithSystem = StartWithSystem,
            Language = Language,
        };
        copy.Clamp();
        return copy;
    }
}
