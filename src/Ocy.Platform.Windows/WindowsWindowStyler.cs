using System.Runtime.Versioning;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsWindowStyler : IWindowStyler
{
    private const int GWL_EXSTYLE = -20;

    public bool IsSupported => OperatingSystem.IsWindows();

    public void MakeClickThrough(nint windowHandle)
    {
        if (!IsSupported || windowHandle == 0)
        {
            return;
        }

        try
        {
            nint current = NativeMethods.GetWindowLongPtrW(windowHandle, GWL_EXSTYLE);
            NativeMethods.SetWindowLongPtrW(windowHandle, GWL_EXSTYLE, ExStyle.ClickThrough(current));
        }
        catch
        {
        }
    }
}
