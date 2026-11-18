using Avalonia.Styling;

namespace Ocy.Desktop.Tray;

public static class TraySetSelector
{
    public static bool UseLightSet(bool? isLightTaskbar, ThemeVariant? appTheme)
    {
        if (isLightTaskbar.HasValue)
        {
            return isLightTaskbar.Value;
        }

        return appTheme == ThemeVariant.Light;
    }
}
