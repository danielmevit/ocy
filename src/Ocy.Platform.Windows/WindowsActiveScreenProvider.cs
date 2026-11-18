using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsActiveScreenProvider : IActiveScreenProvider
{
    private const uint MonitorDefaultToNull = 0;
    private const uint MonitorDefaultToNearest = 2;

    public bool IsSupported => OperatingSystem.IsWindows();

    public ScreenRect? GetActiveScreenBounds()
    {
        if (!IsSupported)
        {
            return null;
        }

        try
        {
            nint hwnd = NativeMethods.GetForegroundWindow();
            nint hmon = 0;
            if (hwnd != 0 && hwnd != NativeMethods.GetShellWindow())
            {
                hmon = NativeMethods.MonitorFromWindow(hwnd, MonitorDefaultToNull);
            }

            if (hmon == 0)
            {
                if (!NativeMethods.GetCursorPos(out NativeMethods.POINT pt))
                {
                    return null;
                }

                hmon = NativeMethods.MonitorFromPoint(pt, MonitorDefaultToNearest);
            }

            if (hmon == 0)
            {
                return null;
            }

            var info = new NativeMethods.MONITORINFO();
            info.cbSize = (uint)Marshal.SizeOf(info);
            if (!NativeMethods.GetMonitorInfoW(hmon, ref info))
            {
                return null;
            }

            return RectMath.ToScreenRect(info.rcMonitor.left, info.rcMonitor.top, info.rcMonitor.right, info.rcMonitor.bottom);
        }
        catch
        {
            return null;
        }
    }
}
