using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;

namespace Ocy.Desktop.Layout;

public static class ScreenQuery
{
    public static IReadOnlyList<ScreenInfo> GetScreens(Window? fallbackWindow = null)
    {
        if (fallbackWindow?.Screens != null && fallbackWindow.Screens.All.Count > 0)
        {
            return ToScreenInfos(fallbackWindow.Screens);
        }

        if (Application.Current?.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var openWin = desktop.Windows.FirstOrDefault(w => w.Screens != null && w.Screens.All.Count > 0);
            if (openWin != null)
            {
                return ToScreenInfos(openWin.Screens);
            }
        }

        try
        {
            var temp = new Window();
            try
            {
                if (temp.Screens != null && temp.Screens.All.Count > 0)
                {
                    return ToScreenInfos(temp.Screens);
                }
            }
            finally
            {
                temp.Close();
            }
        }
        catch
        {
            // In headless/test or platform without windowing manager, fall back to safe default
        }

        return [new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true)];
    }

    private static List<ScreenInfo> ToScreenInfos(Screens screens)
    {
        var list = new List<ScreenInfo>(screens.All.Count);
        foreach (var s in screens.All)
        {
            list.Add(new ScreenInfo(s.Bounds, s.Scaling, s.IsPrimary, s.WorkingArea));
        }
        return list;
    }
}
