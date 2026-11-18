using System.Runtime.Versioning;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsFocusStateProvider : IFocusStateProvider
{
    public bool IsSupported => OperatingSystem.IsWindows();

    public FocusState GetState()
    {
        if (!IsSupported)
        {
            return FocusState.Unknown;
        }

        int hresult = NativeMethods.SHQueryUserNotificationState(out int state);
        return FocusStateMapper.Map(hresult, state);
    }
}
