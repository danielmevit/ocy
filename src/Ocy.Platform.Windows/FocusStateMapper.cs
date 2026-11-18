using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

public static class FocusStateMapper
{
    public static FocusState Map(int hresult, int quns)
    {
        if (hresult < 0)
        {
            return FocusState.Unknown;
        }

        return quns switch
        {
            1 => FocusState.NotPresent,
            2 => FocusState.FullscreenApp,
            3 => FocusState.FullscreenApp,
            4 => FocusState.Presentation,
            5 => FocusState.Normal,
            6 => FocusState.QuietTime,
            7 => FocusState.FullscreenApp,
            _ => FocusState.Unknown,
        };
    }
}
