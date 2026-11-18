using Ocy.Abstractions;

namespace Ocy.Desktop.Layout;

public static class ActiveScreenSelector
{
    public static int Select(IReadOnlyList<ScreenInfo> screens, ScreenRect? active)
    {
        ArgumentNullException.ThrowIfNull(screens);

        if (screens.Count == 0)
        {
            return 0;
        }

        if (active.HasValue)
        {
            var act = active.Value;

            // 1. Prefer the screen whose physical bounds equal active.
            for (var i = 0; i < screens.Count; i++)
            {
                var bounds = screens[i].Bounds;
                if (bounds.X == act.X &&
                    bounds.Y == act.Y &&
                    bounds.Width == act.Width &&
                    bounds.Height == act.Height)
                {
                    return i;
                }
            }

            // 2. Otherwise the screen containing the centre of active.
            var centerX = act.X + act.Width / 2;
            var centerY = act.Y + act.Height / 2;

            for (var i = 0; i < screens.Count; i++)
            {
                var bounds = screens[i].Bounds;
                if (centerX >= bounds.X &&
                    centerX < bounds.X + bounds.Width &&
                    centerY >= bounds.Y &&
                    centerY < bounds.Y + bounds.Height)
                {
                    return i;
                }
            }
        }

        // 3. Otherwise the primary screen.
        for (var i = 0; i < screens.Count; i++)
        {
            if (screens[i].IsPrimary)
            {
                return i;
            }
        }

        return 0;
    }
}
