using System.Runtime.Versioning;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsIdleProvider : IUserIdleProvider
{
    public bool IsSupported => OperatingSystem.IsWindows();

    public TimeSpan GetIdleTime()
    {
        if (!IsSupported)
        {
            return TimeSpan.Zero;
        }

        var info = new NativeMethods.LASTINPUTINFO
        {
            cbSize = (uint)System.Runtime.InteropServices.Marshal.SizeOf<NativeMethods.LASTINPUTINFO>(),
        };

        if (!NativeMethods.GetLastInputInfo(ref info))
        {
            return TimeSpan.Zero;
        }

        return IdleMath.Compute(unchecked((uint)Environment.TickCount), info.dwTime);
    }
}
