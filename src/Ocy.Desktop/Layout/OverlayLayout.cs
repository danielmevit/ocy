using Avalonia;

namespace Ocy.Desktop.Layout;

public readonly record struct ScreenInfo
{
    public PixelRect Bounds { get; init; }
    public double Scaling { get; init; }
    public bool IsPrimary { get; init; }
    public PixelRect? WorkingArea { get; init; }

    public PixelRect EffectiveWorkingArea => WorkingArea ?? Bounds;

    public ScreenInfo(PixelRect bounds, double scaling, bool isPrimary = false, PixelRect? workingArea = null)
    {
        Bounds = bounds;
        Scaling = scaling;
        IsPrimary = isPrimary;
        WorkingArea = workingArea;
    }

    public ScreenInfo(int x, int y, int width, int height, double scaling, bool isPrimary = false)
        : this(new PixelRect(x, y, width, height), scaling, isPrimary, null)
    {
    }

    public ScreenInfo(int x, int y, int width, int height, double scaling, bool isPrimary, PixelRect workingArea)
        : this(new PixelRect(x, y, width, height), scaling, isPrimary, workingArea)
    {
    }
}

public readonly record struct OverlayBounds(
    PixelPoint Position,
    Size DipSize,
    bool IsPrimary);

public static class OverlayLayout
{
    public static Size ComputeDipSize(int physicalWidth, int physicalHeight, double scaling)
    {
        var scale = scaling > 0 ? scaling : 1.0;
        return new Size(physicalWidth / scale, physicalHeight / scale);
    }

    public static Size ComputeDipSize(PixelSize physicalSize, double scaling) =>
        ComputeDipSize(physicalSize.Width, physicalSize.Height, scaling);

    public static IReadOnlyList<OverlayBounds> Compute(IReadOnlyList<ScreenInfo> screens)
    {
        ArgumentNullException.ThrowIfNull(screens);

        if (screens.Count == 0)
        {
            return Array.Empty<OverlayBounds>();
        }

        var result = new List<OverlayBounds>(screens.Count);
        foreach (var screen in screens)
        {
            var size = ComputeDipSize(screen.Bounds.Width, screen.Bounds.Height, screen.Scaling);
            var position = new PixelPoint(screen.Bounds.X, screen.Bounds.Y);

            result.Add(new OverlayBounds(position, size, screen.IsPrimary));
        }

        return result;
    }

    public static PixelPoint ComputeCornerPosition(
        ScreenInfo screen,
        Size windowDipSize,
        double marginDip = 16.0)
    {
        var scaling = screen.Scaling > 0 ? screen.Scaling : 1.0;
        var workArea = screen.EffectiveWorkingArea;

        var totalWidthPhysical = (int)Math.Round((windowDipSize.Width + marginDip) * scaling);
        var totalHeightPhysical = (int)Math.Round((windowDipSize.Height + marginDip) * scaling);

        var x = (workArea.X + workArea.Width) - totalWidthPhysical;
        var y = (workArea.Y + workArea.Height) - totalHeightPhysical;

        return new PixelPoint(x, y);
    }
}
