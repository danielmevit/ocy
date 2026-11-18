namespace Ocy.Platform.Windows;

public static class IdleMath
{
    public static TimeSpan Compute(uint nowTicks, uint lastInputTicks)
    {
        return TimeSpan.FromMilliseconds(unchecked(nowTicks - lastInputTicks));
    }
}
