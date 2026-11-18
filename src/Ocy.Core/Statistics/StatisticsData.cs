namespace Ocy.Core.Statistics;

public sealed class StatisticsData
{
    public const int MaxRetentionDays = 400;

    public int SchemaVersion { get; set; } = 1;

    public List<DailyStatistics> Days { get; set; } = new();

    public DailyStatistics GetOrCreate(DateOnly date)
    {
        var existing = Days.FirstOrDefault(d => d.Date == date);
        if (existing != null)
        {
            return existing;
        }

        var created = new DailyStatistics { Date = date };
        Days.Add(created);
        return created;
    }

    public void PruneRetention(DateOnly today, int maxDays = MaxRetentionDays)
    {
        var cutoff = today.AddDays(-maxDays);
        Days.RemoveAll(d => d.Date < cutoff);
    }

    public StatisticsData Clone() => new()
    {
        SchemaVersion = SchemaVersion,
        Days = Days.Select(d => d.Clone()).ToList(),
    };
}
