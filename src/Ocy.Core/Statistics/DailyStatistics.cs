namespace Ocy.Core.Statistics;

public sealed class DailyStatistics
{
    public DateOnly Date { get; set; }

    public int ShortCompleted { get; set; }

    public int ShortSkipped { get; set; }

    public int LongCompleted { get; set; }

    public int LongSkipped { get; set; }

    public int LongPostponed { get; set; }

    public int AbsencesCredited { get; set; }

    public int ActiveMinutes { get; set; }

    public DailyStatistics Clone() => new()
    {
        Date = Date,
        ShortCompleted = ShortCompleted,
        ShortSkipped = ShortSkipped,
        LongCompleted = LongCompleted,
        LongSkipped = LongSkipped,
        LongPostponed = LongPostponed,
        AbsencesCredited = AbsencesCredited,
        ActiveMinutes = ActiveMinutes,
    };
}
