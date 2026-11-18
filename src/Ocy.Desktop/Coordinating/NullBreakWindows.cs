using Ocy.Core.Engine;

namespace Ocy.Desktop.Coordinating;

public sealed class NullBreakWindows : IBreakWindows
{
    public static readonly NullBreakWindows Instance = new();

    public void ShowShortBreak(BreakInfo info)
    {
    }

    public void ShowWarning(TimeSpan countdown)
    {
    }

    public void ShowLongBreak(BreakInfo info)
    {
    }

    public void UpdateCountdown(EngineSnapshot snapshot)
    {
    }

    public void CloseAll()
    {
    }
}
