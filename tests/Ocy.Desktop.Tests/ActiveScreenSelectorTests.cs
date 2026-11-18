using Ocy.Abstractions;
using Ocy.Desktop.Layout;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class ActiveScreenSelectorTests
{
    // Owner's real layout:
    // Screen 0: primary 1920x1080 @(0,0)
    // Screen 1: 1920x1200 @(-3840,0)
    // Screen 2: 1920x1200 @(-1920,0)
    private readonly ScreenInfo[] _realLayout =
    [
        new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true),
        new ScreenInfo(-3840, 0, 1920, 1200, 1.0, isPrimary: false),
        new ScreenInfo(-1920, 0, 1920, 1200, 1.0, isPrimary: false)
    ];

    [Fact]
    public void Select_WhenActiveIsNull_ReturnsPrimaryScreen()
    {
        var index = ActiveScreenSelector.Select(_realLayout, null);
        Assert.Equal(0, index);
    }

    [Fact]
    public void Select_ExactMatch_SecondaryScreen1()
    {
        var active = new ScreenRect(-3840, 0, 1920, 1200);
        var index = ActiveScreenSelector.Select(_realLayout, active);
        Assert.Equal(1, index);
    }

    [Fact]
    public void Select_ExactMatch_SecondaryScreen2()
    {
        var active = new ScreenRect(-1920, 0, 1920, 1200);
        var index = ActiveScreenSelector.Select(_realLayout, active);
        Assert.Equal(2, index);
    }

    [Fact]
    public void Select_ExactMatch_PrimaryScreen()
    {
        var active = new ScreenRect(0, 0, 1920, 1080);
        var index = ActiveScreenSelector.Select(_realLayout, active);
        Assert.Equal(0, index);
    }

    [Fact]
    public void Select_CenterMatch_WindowOnSecondaryScreen1()
    {
        // Window on screen 1: X: -3000, Width: 800 -> CenterX: -2600 (within [-3840, -1920))
        var activeWindow = new ScreenRect(-3000, 100, 800, 600);
        var index = ActiveScreenSelector.Select(_realLayout, activeWindow);
        Assert.Equal(1, index);
    }

    [Fact]
    public void Select_CenterMatch_WindowOnSecondaryScreen2()
    {
        // Window on screen 2: X: -1000, Width: 800 -> CenterX: -600 (within [-1920, 0))
        var activeWindow = new ScreenRect(-1000, 100, 800, 600);
        var index = ActiveScreenSelector.Select(_realLayout, activeWindow);
        Assert.Equal(2, index);
    }

    [Fact]
    public void Select_CenterMatch_WindowOnPrimaryScreen()
    {
        // Window on primary: X: 100, Width: 800 -> CenterX: 500 (within [0, 1920))
        var activeWindow = new ScreenRect(100, 100, 800, 600);
        var index = ActiveScreenSelector.Select(_realLayout, activeWindow);
        Assert.Equal(0, index);
    }

    [Fact]
    public void Select_WhenCenterIsOffScreen_FallsBackToPrimaryScreen()
    {
        // Coordinates outside any monitor
        var offscreen = new ScreenRect(9999, 9999, 500, 500);
        var index = ActiveScreenSelector.Select(_realLayout, offscreen);
        Assert.Equal(0, index);
    }

    [Fact]
    public void Select_WhenNoPrimaryMarked_FallsBackToFirstScreen()
    {
        var screensWithoutPrimary = new[]
        {
            new ScreenInfo(1920, 0, 1920, 1080, 1.0, isPrimary: false),
            new ScreenInfo(3840, 0, 1920, 1080, 1.0, isPrimary: false)
        };

        var index = ActiveScreenSelector.Select(screensWithoutPrimary, null);
        Assert.Equal(0, index);
    }

    [Fact]
    public void Select_EmptyScreens_ReturnsZero()
    {
        var index = ActiveScreenSelector.Select([], null);
        Assert.Equal(0, index);
    }
}
