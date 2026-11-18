using Microsoft.Extensions.Time.Testing;
using Ocy.Core.Persistence;
using Ocy.Core.Statistics;
using Xunit;

namespace Ocy.Core.Tests;

public sealed class StatisticsTests : IDisposable
{
    private readonly string _testDir;
    private readonly FakeTimeProvider _timeProvider;

    public StatisticsTests()
    {
        _testDir = Path.Combine(Path.GetTempPath(), "ocy-tests-stats-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_testDir);
        _timeProvider = new FakeTimeProvider(new DateTimeOffset(2026, 11, 17, 10, 0, 0, TimeSpan.Zero));
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
    public void DailyBuckets_KeyedByLocalDate()
    {
        var stats = new StatisticsData();
        var today = new DateOnly(2026, 11, 17);
        var tomorrow = new DateOnly(2026, 11, 18);

        var bucketToday = stats.GetOrCreate(today);
        bucketToday.ShortCompleted += 3;
        bucketToday.LongCompleted += 1;
        bucketToday.ActiveMinutes += 45;

        var bucketTomorrow = stats.GetOrCreate(tomorrow);
        bucketTomorrow.ShortSkipped += 1;
        bucketTomorrow.ActiveMinutes += 10;

        Assert.Equal(2, stats.Days.Count);
        Assert.Equal(3, stats.GetOrCreate(today).ShortCompleted);
        Assert.Equal(1, stats.GetOrCreate(today).LongCompleted);
        Assert.Equal(45, stats.GetOrCreate(today).ActiveMinutes);

        Assert.Equal(1, stats.GetOrCreate(tomorrow).ShortSkipped);
        Assert.Equal(10, stats.GetOrCreate(tomorrow).ActiveMinutes);
    }

    [Fact]
    public void Retention_PrunesBucketsOlderThan400Days()
    {
        var stats = new StatisticsData();
        var today = new DateOnly(2026, 11, 17);

        var ancientDate = today.AddDays(-405);
        var borderDate = today.AddDays(-400);
        var recentDate = today.AddDays(-10);

        stats.GetOrCreate(ancientDate).ShortCompleted = 10;
        stats.GetOrCreate(borderDate).ShortCompleted = 5;
        stats.GetOrCreate(recentDate).ShortCompleted = 8;
        stats.GetOrCreate(today).ShortCompleted = 2;

        Assert.Equal(4, stats.Days.Count);

        stats.PruneRetention(today, maxDays: 400);

        // ancientDate (405 days ago) is pruned; borderDate (400 days) and newer are retained
        Assert.Equal(3, stats.Days.Count);
        Assert.Null(stats.Days.FirstOrDefault(d => d.Date == ancientDate));
        Assert.NotNull(stats.Days.FirstOrDefault(d => d.Date == borderDate));
        Assert.NotNull(stats.Days.FirstOrDefault(d => d.Date == recentDate));
        Assert.NotNull(stats.Days.FirstOrDefault(d => d.Date == today));
    }

    [Fact]
    public void Persistence_AtomicSaveAndLoadRoundTrip()
    {
        var store = new AppDataStore(_testDir, _timeProvider);
        var stats = new StatisticsData();
        var today = new DateOnly(2026, 11, 17);

        var bucket = stats.GetOrCreate(today);
        bucket.ShortCompleted = 5;
        bucket.ShortSkipped = 1;
        bucket.LongCompleted = 2;
        bucket.LongSkipped = 0;
        bucket.LongPostponed = 3;
        bucket.AbsencesCredited = 1;
        bucket.ActiveMinutes = 120;

        store.SaveStats(stats);

        Assert.True(File.Exists(store.StatsPath));
        Assert.False(File.Exists(store.StatsPath + ".tmp"));

        var loaded = store.LoadStats();
        Assert.Single(loaded.Days);
        var loadedBucket = loaded.GetOrCreate(today);
        Assert.Equal(5, loadedBucket.ShortCompleted);
        Assert.Equal(1, loadedBucket.ShortSkipped);
        Assert.Equal(2, loadedBucket.LongCompleted);
        Assert.Equal(3, loadedBucket.LongPostponed);
        Assert.Equal(1, loadedBucket.AbsencesCredited);
        Assert.Equal(120, loadedBucket.ActiveMinutes);
    }
}
