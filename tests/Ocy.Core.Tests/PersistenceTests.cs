using Microsoft.Extensions.Time.Testing;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Persistence;
using Ocy.Core.Platform;
using Ocy.Core.Settings;
using Xunit;

namespace Ocy.Core.Tests;

public sealed class PersistenceTests : IDisposable
{
    private readonly string _testDir;
    private readonly FakeTimeProvider _timeProvider;

    public PersistenceTests()
    {
        _testDir = Path.Combine(Path.GetTempPath(), "ocy-tests-persist-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_testDir);
        _timeProvider = new FakeTimeProvider(new DateTimeOffset(2026, 11, 17, 8, 0, 0, TimeSpan.Zero));
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
    public void EngineState_RoundTripSaveAndLoad()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        var state = new EngineState
        {
            SinceShortBreak = TimeSpan.FromMinutes(8),
            SinceLongBreak = TimeSpan.FromMinutes(42),
            PostponesUsed = 2,
            PausedUntilUtc = _timeProvider.GetUtcNow().AddHours(1),
            LastTickUtc = _timeProvider.GetUtcNow(),
        };

        store.SaveState(state);

        Assert.True(File.Exists(store.StatePath));
        Assert.False(File.Exists(store.StatePath + ".tmp"));

        var loaded = store.LoadState();
        Assert.Equal(TimeSpan.FromMinutes(8), loaded.SinceShortBreak);
        Assert.Equal(TimeSpan.FromMinutes(42), loaded.SinceLongBreak);
        Assert.Equal(2, loaded.PostponesUsed);
        Assert.Equal(state.PausedUntilUtc, loaded.PausedUntilUtc);
        Assert.Equal(state.LastTickUtc, loaded.LastTickUtc);
    }

    [Fact]
    public void CorruptStateFile_BackedUpAndDefaultsReturned()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        File.WriteAllText(store.StatePath, "{ invalid json state !!!");

        var loaded = store.LoadState();
        Assert.NotNull(loaded);
        Assert.Equal(TimeSpan.Zero, loaded.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, loaded.SinceLongBreak);

        var bakFiles = Directory.GetFiles(_testDir, "state.json.bak-*");
        Assert.Single(bakFiles);
        Assert.Contains("20261117T080000Z", Path.GetFileName(bakFiles[0]));
    }

    [Fact]
    public void EngineRestart_AfterLongGap_CreditsLongBreak()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        var settings = new OcySettings();
        settings.ShortBreaks.DurationSeconds = 15;
        settings.LongBreaks.DurationMinutes = 5;

        var t0 = _timeProvider.GetUtcNow();
        var savedState = new EngineState
        {
            SinceShortBreak = TimeSpan.FromMinutes(5),
            SinceLongBreak = TimeSpan.FromMinutes(35),
            LastTickUtc = t0,
        };
        store.SaveState(savedState);

        // Advance fake clock by 10 minutes (gap = 10 min >= 5 min long break duration)
        _timeProvider.Advance(TimeSpan.FromMinutes(10));

        BreakKind? creditedKind = null;
        using var engine = new BreakEngine(
            settings: settings,
            platformServices: UnsupportedPlatformServices.Instance,
            timeProvider: _timeProvider,
            dataStore: store);

        engine.AbsenceCredited += kind => creditedKind = kind;
        engine.Tick();

        Assert.Equal(BreakKind.Long, creditedKind);
        Assert.Equal(TimeSpan.Zero, engine.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, engine.SinceLongBreak);
        Assert.Equal(1, engine.Statistics.GetOrCreate(new DateOnly(2026, 11, 17)).AbsencesCredited);
    }

    [Fact]
    public void EngineRestart_AfterShortGap_ResetsOnlyShortCounter()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        var settings = new OcySettings();
        settings.ShortBreaks.DurationSeconds = 15;
        settings.LongBreaks.DurationMinutes = 5;

        var t0 = _timeProvider.GetUtcNow();
        var savedState = new EngineState
        {
            SinceShortBreak = TimeSpan.FromMinutes(5),
            SinceLongBreak = TimeSpan.FromMinutes(35),
            LastTickUtc = t0,
        };
        store.SaveState(savedState);

        // Advance fake clock by 30 seconds (30s >= 15s short duration, but < 5 min long duration)
        _timeProvider.Advance(TimeSpan.FromSeconds(30));

        BreakKind? creditedKind = null;
        using var engine = new BreakEngine(
            settings: settings,
            platformServices: UnsupportedPlatformServices.Instance,
            timeProvider: _timeProvider,
            dataStore: store);

        engine.AbsenceCredited += kind => creditedKind = kind;
        engine.Tick();

        Assert.Null(creditedKind);
        Assert.Equal(TimeSpan.Zero, engine.SinceShortBreak);
        Assert.Equal(TimeSpan.FromMinutes(35), engine.SinceLongBreak);
    }

    [Fact]
    public void DefectD_DirectoryAtSettingsPath_LoadSettingsReturnsDefaults_WithoutRenaming()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        Directory.CreateDirectory(store.SettingsPath);

        var settings = store.LoadSettings();

        Assert.NotNull(settings);
        // Defaults returned
        Assert.Equal(10, settings.ShortBreaks.IntervalMinutes);
        Assert.Equal(60, settings.LongBreaks.IntervalMinutes);

        // Directory still exists and was not renamed/backed up
        Assert.True(Directory.Exists(store.SettingsPath));
        var bakFiles = Directory.GetFileSystemEntries(_testDir, "settings.json.bak-*");
        Assert.Empty(bakFiles);
    }

    [Fact]
    public void DefectD_DirectoryAtStateAndStatsPath_LoadReturnsDefaults_WithoutRenaming()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        Directory.CreateDirectory(store.StatePath);
        Directory.CreateDirectory(store.StatsPath);

        var state = store.LoadState();
        var stats = store.LoadStats();

        Assert.NotNull(state);
        Assert.Equal(TimeSpan.Zero, state.SinceShortBreak);
        Assert.Equal(TimeSpan.Zero, state.SinceLongBreak);
        Assert.True(Directory.Exists(store.StatePath));

        Assert.NotNull(stats);
        Assert.Empty(stats.Days);
        Assert.True(Directory.Exists(store.StatsPath));

        var bakFiles = Directory.GetFileSystemEntries(_testDir, "*.bak-*");
        Assert.Empty(bakFiles);
    }
}
