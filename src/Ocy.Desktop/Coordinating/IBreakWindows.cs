using Ocy.Core.Engine;

namespace Ocy.Desktop.Coordinating;

public interface IBreakWindows
{
    void ShowShortBreak(BreakInfo info);

    void ShowWarning(TimeSpan countdown);

    void ShowLongBreak(BreakInfo info);

    void UpdateCountdown(EngineSnapshot snapshot);

    void CloseAll();
}
