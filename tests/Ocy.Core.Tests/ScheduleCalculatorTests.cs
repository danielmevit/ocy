using Ocy.Core.Engine;
using Ocy.Core.Settings;
using Xunit;

namespace Ocy.Core.Tests;

public sealed class ScheduleCalculatorTests
{
    [Fact]
    public void Calculate_WhenCountersZero_ReturnsFullIntervals()
    {
        var settings = new OcySettings();
        settings.ShortBreaks.IntervalMinutes = 10;
        settings.LongBreaks.IntervalMinutes = 60;
        settings.LongBreaks.WarningSeconds = 60;

        var shortDue = ScheduleCalculator.GetTimeUntilShortBreak(settings, TimeSpan.Zero);
        var longDue = ScheduleCalculator.GetTimeUntilLongBreak(settings, TimeSpan.Zero);
        var warningDue = ScheduleCalculator.GetTimeUntilLongBreakWarning(settings, TimeSpan.Zero);

        Assert.Equal(TimeSpan.FromMinutes(10), shortDue);
        Assert.Equal(TimeSpan.FromMinutes(60), longDue);
        Assert.Equal(TimeSpan.FromMinutes(59), warningDue);
    }

    [Fact]
    public void Calculate_WithElapsedTime_ReturnsRemainingTime()
    {
        var settings = new OcySettings();
        settings.ShortBreaks.IntervalMinutes = 10;
        settings.LongBreaks.IntervalMinutes = 60;
        settings.LongBreaks.WarningSeconds = 60;

        var shortDue = ScheduleCalculator.GetTimeUntilShortBreak(settings, TimeSpan.FromMinutes(4));
        var longDue = ScheduleCalculator.GetTimeUntilLongBreak(settings, TimeSpan.FromMinutes(25));
        var warningDue = ScheduleCalculator.GetTimeUntilLongBreakWarning(settings, TimeSpan.FromMinutes(25));

        Assert.Equal(TimeSpan.FromMinutes(6), shortDue);
        Assert.Equal(TimeSpan.FromMinutes(35), longDue);
        Assert.Equal(TimeSpan.FromMinutes(34), warningDue);
    }

    [Fact]
    public void Calculate_WhenElapsedExceedsInterval_ReturnsZero()
    {
        var settings = new OcySettings();
        settings.ShortBreaks.IntervalMinutes = 10;

        var shortDue = ScheduleCalculator.GetTimeUntilShortBreak(settings, TimeSpan.FromMinutes(12));
        Assert.Equal(TimeSpan.Zero, shortDue);
    }

    [Fact]
    public void Calculate_WhenBreakDisabled_ReturnsNull()
    {
        var settings = new OcySettings();
        settings.ShortBreaks.Enabled = false;
        settings.LongBreaks.Enabled = false;

        var shortDue = ScheduleCalculator.GetTimeUntilShortBreak(settings, TimeSpan.Zero);
        var longDue = ScheduleCalculator.GetTimeUntilLongBreak(settings, TimeSpan.Zero);
        var warningDue = ScheduleCalculator.GetTimeUntilLongBreakWarning(settings, TimeSpan.Zero);

        Assert.Null(shortDue);
        Assert.Null(longDue);
        Assert.Null(warningDue);
    }

    [Fact]
    public void Suppression_WhenLongBreakDueWithin2Minutes_ReturnsTrue()
    {
        var settings = new OcySettings();
        settings.LongBreaks.IntervalMinutes = 60;

        // Long break due in 1 minute
        var suppressed = ScheduleCalculator.IsShortBreakSuppressedByLongBreak(settings, TimeSpan.FromMinutes(59));
        Assert.True(suppressed);

        // Long break due in exactly 2 minutes
        var exactly2Min = ScheduleCalculator.IsShortBreakSuppressedByLongBreak(settings, TimeSpan.FromMinutes(58));
        Assert.True(exactly2Min);

        // Long break due in 5 minutes
        var notSuppressed = ScheduleCalculator.IsShortBreakSuppressedByLongBreak(settings, TimeSpan.FromMinutes(55));
        Assert.False(notSuppressed);
    }
}
