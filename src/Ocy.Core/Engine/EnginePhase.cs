namespace Ocy.Core.Engine;

public enum EnginePhase
{
    Working = 1,
    LongBreakWarning = 2,
    ShortBreak = 3,
    LongBreak = 4,
    Deferred = 5,
    Paused = 6,
    Away = 7,
}
