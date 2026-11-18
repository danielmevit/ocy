namespace Ocy.Platform.Windows;

/// <summary>
/// Pure mapping for the Windows taskbar theme registry value
/// (no Windows calls; unit-tested on Linux).
/// </summary>
public static class TaskbarThemeMath
{
    /// <summary>
    /// Maps the <c>SystemUsesLightTheme</c> DWORD: 1 = light, 0 = dark. Missing (null), any other
    /// value or any non-<see cref="int"/> type means unknown.
    /// </summary>
    public static bool? FromRegistryValue(object? value)
    {
        return value is int number
            ? number switch
            {
                1 => true,
                0 => false,
                _ => null,
            }
            : null;
    }
}
