namespace Ocy.Platform.Windows;

/// <summary>Pure math for the Windows "Animation effects" preference (no Windows calls; unit-tested on Linux).</summary>
public static class MotionMath
{
    /// <summary>
    /// False when the call failed (unknown means animations allowed), true when the value is 0
    /// ("Animation effects" off), false otherwise.
    /// </summary>
    public static bool Reduced(bool callSucceeded, int clientAreaAnimation)
    {
        return callSucceeded && clientAreaAnimation == 0;
    }
}
