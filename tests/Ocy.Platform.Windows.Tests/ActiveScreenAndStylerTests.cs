using Ocy.Abstractions;
using Ocy.Platform.Windows;
using Xunit;

namespace Ocy.Platform.Windows.Tests;

public class RectMathTests
{
    [Fact]
    public void ToScreenRect_ConvertsLTRB()
    {
        Assert.Equal(new ScreenRect(100, 200, 1920, 1080), RectMath.ToScreenRect(100, 200, 2020, 1280));
    }

    [Fact]
    public void ToScreenRect_NegativeCoordinates_Work()
    {
        Assert.Equal(new ScreenRect(-3840, 0, 1920, 1200), RectMath.ToScreenRect(-3840, 0, -1920, 1200));
    }

    [Fact]
    public void ToScreenRect_AllNegative_Works()
    {
        Assert.Equal(new ScreenRect(-3840, -1200, 1920, 1200), RectMath.ToScreenRect(-3840, -1200, -1920, 0));
    }

    [Fact]
    public void ToScreenRect_ZeroSizedRect_Works()
    {
        Assert.Equal(new ScreenRect(5, 5, 0, 0), RectMath.ToScreenRect(5, 5, 5, 5));
    }
}

public class ExStyleTests
{
    [Theory]
    [InlineData(0)]
    [InlineData(0x10)]
    [InlineData(0x100)]
    [InlineData(unchecked((int)0x80000000))]
    public void ClickThrough_KeepsExistingBits(int current)
    {
        nint expected = current | 0x20 | 0x80 | 0x80000 | 0x08000000;
        Assert.Equal(expected, ExStyle.ClickThrough(current));
    }

    [Fact]
    public void ClickThrough_AllFourFlagsSet()
    {
        nint result = ExStyle.ClickThrough(0);
        Assert.Equal(0x20 | 0x80 | 0x80000 | 0x08000000, result);
        Assert.Equal(0x080800A0, result);
    }

    [Fact]
    public void ClickThrough_Idempotent()
    {
        nint once = ExStyle.ClickThrough(0x4);
        nint twice = ExStyle.ClickThrough(once);
        Assert.Equal(once, twice);
    }
}
