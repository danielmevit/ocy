using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

public static class RectMath
{
    public static ScreenRect ToScreenRect(int left, int top, int right, int bottom)
    {
        return new ScreenRect(left, top, right - left, bottom - top);
    }
}
