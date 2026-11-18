using System.Text.Json;
using Microsoft.Extensions.Time.Testing;
using Ocy.Core.Persistence;
using Ocy.Core.Settings;
using Xunit;

namespace Ocy.Core.Tests;

public sealed class SettingsTests : IDisposable
{
    private readonly string _testDir;
    private readonly FakeTimeProvider _timeProvider;

    public SettingsTests()
    {
        _testDir = Path.Combine(Path.GetTempPath(), "ocy-tests-settings-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_testDir);
        _timeProvider = new FakeTimeProvider(new DateTimeOffset(2026, 11, 17, 12, 0, 0, TimeSpan.Zero));
    }

    public void Dispose()
    {
        if (Directory.Exists(_testDir))
        {
            try
            {
                Directory.Delete(_testDir, recursive: true);
            }
            catch
            {
                // Best effort cleanup
            }
        }
    }

    [Fact]
    public void Defaults_MatchArchitectureSpecification()
    {
        var settings = new OcySettings();
        settings.Clamp();

        Assert.Equal(1, settings.SchemaVersion);

        // Short breaks
        Assert.True(settings.ShortBreaks.Enabled);
        Assert.Equal(10, settings.ShortBreaks.IntervalMinutes);
        Assert.Equal(15, settings.ShortBreaks.DurationSeconds);
        Assert.False(settings.ShortBreaks.Fullscreen);

        // Long breaks
        Assert.True(settings.LongBreaks.Enabled);
        Assert.Equal(60, settings.LongBreaks.IntervalMinutes);
        Assert.Equal(5, settings.LongBreaks.DurationMinutes);
        Assert.Equal(60, settings.LongBreaks.WarningSeconds);
        Assert.Equal(3, settings.LongBreaks.PostponeMinutes);
        Assert.Equal(3, settings.LongBreaks.MaxPostpones);

        // General
        Assert.False(settings.StrictMode);
        Assert.False(settings.Sounds);
        Assert.True(settings.InactivityTracking);
        Assert.Equal(120, settings.AwayThresholdSeconds);
        Assert.True(settings.PauseDuringFullscreen);
        Assert.True(settings.HasWindowNearby);
        Assert.True(settings.StatusNotifications);
        Assert.False(settings.StartWithSystem);
        Assert.Equal("en", settings.Language);
    }

    [Fact]
    public void Clamping_EnforcesAllowedRanges()
    {
        var settings = new OcySettings
        {
            ShortBreaks = new ShortBreakSettings
            {
                IntervalMinutes = 1, // min 2
                DurationSeconds = 200, // max 120
            },
            LongBreaks = new LongBreakSettings
            {
                IntervalMinutes = 500, // max 240
                DurationMinutes = 0, // min 1
                WarningSeconds = 500, // max 300
                PostponeMinutes = 50, // max 30
                MaxPostpones = 20, // max 10
            },
            AwayThresholdSeconds = 10, // min 30
            Language = "   ", // fallback "en"
        };

        settings.Clamp();

        Assert.Equal(2, settings.ShortBreaks.IntervalMinutes);
        Assert.Equal(120, settings.ShortBreaks.DurationSeconds);
        Assert.Equal(240, settings.LongBreaks.IntervalMinutes);
        Assert.Equal(1, settings.LongBreaks.DurationMinutes);
        Assert.Equal(300, settings.LongBreaks.WarningSeconds);
        Assert.Equal(30, settings.LongBreaks.PostponeMinutes);
        Assert.Equal(10, settings.LongBreaks.MaxPostpones);
        Assert.Equal(30, settings.AwayThresholdSeconds);
        Assert.Equal("en", settings.Language);
    }

    [Fact]
    public void LoadSettings_WhenMissing_ReturnsDefaults()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        var settings = store.LoadSettings();

        Assert.NotNull(settings);
        Assert.Equal(10, settings.ShortBreaks.IntervalMinutes);
        Assert.Equal(60, settings.LongBreaks.IntervalMinutes);
    }

    [Fact]
    public void LoadSettings_IgnoresUnknownKeys()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        var json = """
        {
            "schemaVersion": 1,
            "futureUnrecognizedProperty": 9999,
            "anotherUnknown": "abc",
            "shortBreaks": {
                "enabled": true,
                "intervalMinutes": 15,
                "durationSeconds": 25,
                "extraField": true
            }
        }
        """;
        File.WriteAllText(store.SettingsPath, json);

        var loaded = store.LoadSettings();

        Assert.NotNull(loaded);
        Assert.Equal(15, loaded.ShortBreaks.IntervalMinutes);
        Assert.Equal(25, loaded.ShortBreaks.DurationSeconds);
    }

    [Fact]
    public void LoadSettings_CorruptFile_IsBackedUpAndDefaultsUsed()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        File.WriteAllText(store.SettingsPath, "{ this is not valid json! :::");

        var settings = store.LoadSettings();

        // Defaults returned
        Assert.NotNull(settings);
        Assert.Equal(10, settings.ShortBreaks.IntervalMinutes);

        // Corrupt file renamed to settings.json.bak-<utc-timestamp>
        var bakFiles = Directory.GetFiles(_testDir, "settings.json.bak-*");
        Assert.Single(bakFiles);
        Assert.Contains("20261117T120000Z", Path.GetFileName(bakFiles[0]));
        Assert.Equal("{ this is not valid json! :::", File.ReadAllText(bakFiles[0]));
        Assert.False(File.Exists(store.SettingsPath));
    }

    [Fact]
    public void AtomicSaveRoundTrip_SavesAndLoadsAccurately()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        var settings = new OcySettings
        {
            StrictMode = true,
            Sounds = true,
            AwayThresholdSeconds = 180,
            Language = "de",
        };
        settings.ShortBreaks.IntervalMinutes = 12;
        settings.LongBreaks.IntervalMinutes = 45;

        store.SaveSettings(settings);

        Assert.True(File.Exists(store.SettingsPath));
        Assert.False(File.Exists(store.SettingsPath + ".tmp"));

        var loaded = store.LoadSettings();
        Assert.True(loaded.StrictMode);
        Assert.True(loaded.Sounds);
        Assert.Equal(180, loaded.AwayThresholdSeconds);
        Assert.Equal("de", loaded.Language);
        Assert.Equal(12, loaded.ShortBreaks.IntervalMinutes);
        Assert.Equal(45, loaded.LongBreaks.IntervalMinutes);
    }
}
