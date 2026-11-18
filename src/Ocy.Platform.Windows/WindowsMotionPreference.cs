using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsMotionPreference : IMotionPreference
{
    private const uint SpiGetClientAreaAnimation = 0x1042;
    private const uint SpiSendNone = 0;

    public bool IsSupported => OperatingSystem.IsWindows();

    public bool PrefersReducedMotion()
    {
        if (!IsSupported)
        {
            return false;
        }

        try
        {
            return NativeMethods.SystemParametersInfoW(SpiGetClientAreaAnimation, 0, out int clientAreaAnimation, SpiSendNone)
                && MotionMath.Reduced(true, clientAreaAnimation);
        }
        catch
        {
            return false;
        }
    }
}
