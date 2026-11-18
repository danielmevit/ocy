using Ocy.Core.Settings;

namespace Ocy.Core.Engine;

public static class ScheduleCalculator
{
    /// <summary>
    /// Calculates remaining active time until the next short break is due.
    /// Returns null if short breaks are disabled.
    /// </summary>
    public static TimeSpan? GetTimeUntilShortBreak(OcySettings settings, TimeSpan sinceShortBreak)
    {
        if (!settings.ShortBreaks.Enabled)
        {
            return null;
        }

        var remaining = settings.ShortBreaks.Interval - sinceShortBreak;
        return remaining < TimeSpan.Zero ? TimeSpan.Zero : remaining;
    }

    /// <summary>
    /// Calculates remaining active time until the next long break is due.
    /// Returns null if long breaks are disabled.
    /// </summary>
    public static TimeSpan? GetTimeUntilLongBreak(OcySettings settings, TimeSpan sinceLongBreak)
    {
        if (!settings.LongBreaks.Enabled)
        {
            return null;
        }

        var remaining = settings.LongBreaks.Interval - sinceLongBreak;
        return remaining < TimeSpan.Zero ? TimeSpan.Zero : remaining;
    }

    /// <summary>
    /// Calculates remaining active time until the next long break warning begins.
    /// Returns null if long breaks are disabled or warning is 0 seconds.
    /// </summary>
    public static TimeSpan? GetTimeUntilLongBreakWarning(OcySettings settings, TimeSpan sinceLongBreak)
    {
        if (!settings.LongBreaks.Enabled || settings.LongBreaks.WarningSeconds <= 0)
        {
            return null;
        }

        var warningAt = settings.LongBreaks.Interval - settings.LongBreaks.WarningDuration;
        var remaining = warningAt - sinceLongBreak;
        return remaining < TimeSpan.Zero ? TimeSpan.Zero : remaining;
    }

    /// <summary>
    /// Returns true if a short break should be suppressed because a long break is due within 2 minutes.
    /// </summary>
    public static bool IsShortBreakSuppressedByLongBreak(OcySettings settings, TimeSpan sinceLongBreak)
    {
        if (!settings.LongBreaks.Enabled)
        {
            return false;
        }

        var timeUntilLong = settings.LongBreaks.Interval - sinceLongBreak;
        return timeUntilLong <= TimeSpan.FromMinutes(2);
    }
}
