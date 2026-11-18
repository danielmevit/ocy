namespace Ocy.Platform.Windows;

public static class ExStyle
{
    private const nint WS_EX_TRANSPARENT = 0x20;
    private const nint WS_EX_TOOLWINDOW = 0x80;
    private const nint WS_EX_LAYERED = 0x80000;
    private const nint WS_EX_NOACTIVATE = 0x08000000;

    public static nint ClickThrough(nint current)
    {
        return current | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE;
    }
}
