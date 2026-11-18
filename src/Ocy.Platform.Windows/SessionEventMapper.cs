using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

public static class SessionEventMapper
{
    public static SessionEventKind? FromSessionSwitch(int reason)
    {
        return reason switch
        {
            7 => SessionEventKind.Locked,
            8 => SessionEventKind.Unlocked,
            6 => SessionEventKind.LoggingOff,
            _ => null,
        };
    }

    public static SessionEventKind? FromPowerMode(int mode)
    {
        return mode switch
        {
            1 => SessionEventKind.Resumed,
            3 => SessionEventKind.Suspending,
            _ => null,
        };
    }
}
