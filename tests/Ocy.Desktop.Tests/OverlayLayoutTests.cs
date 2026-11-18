using Avalonia;
using Ocy.Desktop.Layout;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class OverlayLayoutTests
{
    [Fact]
    public void Compute_SingleScreen_ReturnsExactDipSizeAndPosition()
    {
        var screens = new[]
        {
            new ScreenInfo(new PixelRect(0, 0, 1920, 1080), 1.0, isPrimary: true)
        };

        var layouts = OverlayLayout.Compute(screens);

        Assert.Single(layouts);
        Assert.Equal(new PixelPoint(0, 0), layouts[0].Position);
        Assert.Equal(new Size(1920, 1080), layouts[0].DipSize);
        Assert.True(layouts[0].IsPrimary);
    }

    [Fact]
    public void Compute_MixedDpi_ComputesCorrectDipSizes()
    {
        // 3840x2160 @ 1.5 scaling -> DIP size 2560x1440
        // 1920x1080 @ 1.0 scaling -> DIP size 1920x1080
        var screens = new[]
        {
            new ScreenInfo(new PixelRect(0, 0, 3840, 2160), 1.5, isPrimary: true),
            new ScreenInfo(new PixelRect(3840, 0, 1920, 1080), 1.0, isPrimary: false)
        };

        var layouts = OverlayLayout.Compute(screens);

        Assert.Equal(2, layouts.Count);

        Assert.Equal(new PixelPoint(0, 0), layouts[0].Position);
        Assert.Equal(new Size(2560, 1440), layouts[0].DipSize);
        Assert.True(layouts[0].IsPrimary);

        Assert.Equal(new PixelPoint(3840, 0), layouts[1].Position);
        Assert.Equal(new Size(1920, 1080), layouts[1].DipSize);
        Assert.False(layouts[1].IsPrimary);
    }

    [Fact]
    public void Compute_NegativeCoordinates_PreservesNegativePosition()
    {
        // Secondary screen placed to the left of the primary screen
        var screens = new[]
        {
            new ScreenInfo(new PixelRect(-1920, 0, 1920, 1080), 1.0, isPrimary: false),
            new ScreenInfo(new PixelRect(0, 0, 1920, 1080), 1.0, isPrimary: true)
        };

        var layouts = OverlayLayout.Compute(screens);

        Assert.Equal(2, layouts.Count);

        Assert.Equal(new PixelPoint(-1920, 0), layouts[0].Position);
        Assert.Equal(new Size(1920, 1080), layouts[0].DipSize);
        Assert.False(layouts[0].IsPrimary);

        Assert.Equal(new PixelPoint(0, 0), layouts[1].Position);
        Assert.Equal(new Size(1920, 1080), layouts[1].DipSize);
        Assert.True(layouts[1].IsPrimary);
    }

    [Fact]
    public void ComputeCornerPosition_StandardDpi_CalculatesBottomRightWithMargin()
    {
        // 1920x1080 screen with 40px taskbar at bottom -> WorkingArea 1920x1040
        var screen = new ScreenInfo(
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scaling: 1.0,
            isPrimary: true,
            workingArea: new PixelRect(0, 0, 1920, 1040));

        var pos = OverlayLayout.ComputeCornerPosition(screen, new Size(360, 140), marginDip: 16);

        // 1920 - (360 + 16) = 1544
        // 1040 - (140 + 16) = 884
        Assert.Equal(new PixelPoint(1544, 884), pos);
    }

    [Fact]
    public void ComputeCornerPosition_HighDpiAndOffset_CalculatesPhysicalPixelsAccurately()
    {
        // 4K screen at scale 1.5, working area starting at x = 1920, y = 0, width = 3840, height = 2100
        var screen = new ScreenInfo(
            x: 1920,
            y: 0,
            width: 3840,
            height: 2160,
            scaling: 1.5,
            isPrimary: true,
            workingArea: new PixelRect(1920, 0, 3840, 2100));

        var pos = OverlayLayout.ComputeCornerPosition(screen, new Size(360, 140), marginDip: 16);

        // (360 + 16) * 1.5 = 564 physical pixels
        // (140 + 16) * 1.5 = 234 physical pixels
        // Right edge: 1920 + 3840 = 5760. 5760 - 564 = 5196
        // Bottom edge: 0 + 2100 = 2100. 2100 - 234 = 1866
        Assert.Equal(new PixelPoint(5196, 1866), pos);
    }

    [Fact]
    public void ComputeDipSize_StandardDpi_ReturnsExactSize()
    {
        var size = OverlayLayout.ComputeDipSize(1920, 1080, 1.0);
        Assert.Equal(new Size(1920, 1080), size);
    }

    [Fact]
    public void ComputeDipSize_HighDpi1Point5_ComputesCorrectDip()
    {
        var size = OverlayLayout.ComputeDipSize(3840, 2160, 1.5);
        Assert.Equal(new Size(2560, 1440), size);
    }

    [Fact]
    public void FrameGeometry_Scaling1_Scaling1Point5_NegativeX_ComputesAccurateBounds()
    {
        // 3-monitor setup matching owner's layout:
        // Screen 0: Secondary left-most at (-3840, 0), 1920x1200 @ 1.0 scaling
        // Screen 1: Secondary middle at (-1920, 0), 1920x1200 @ 1.5 scaling -> 1280x800 DIP
        // Screen 2: Primary right at (0, 0), 1920x1080 @ 1.0 scaling
        var screens = new[]
        {
            new ScreenInfo(-3840, 0, 1920, 1200, 1.0, isPrimary: false),
            new ScreenInfo(-1920, 0, 1920, 1200, 1.5, isPrimary: false),
            new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true)
        };

        var layouts = OverlayLayout.Compute(screens);
        Assert.Equal(3, layouts.Count);

        // Screen 0: negative X (-3840), scaling 1.0
        Assert.Equal(new PixelPoint(-3840, 0), layouts[0].Position);
        Assert.Equal(new Size(1920, 1200), layouts[0].DipSize);
        Assert.False(layouts[0].IsPrimary);

        // Screen 1: negative X (-1920), scaling 1.5
        Assert.Equal(new PixelPoint(-1920, 0), layouts[1].Position);
        Assert.Equal(new Size(1280, 800), layouts[1].DipSize);
        Assert.False(layouts[1].IsPrimary);

        // Screen 2: scaling 1.0, primary
        Assert.Equal(new PixelPoint(0, 0), layouts[2].Position);
        Assert.Equal(new Size(1920, 1080), layouts[2].DipSize);
        Assert.True(layouts[2].IsPrimary);
    }
}
