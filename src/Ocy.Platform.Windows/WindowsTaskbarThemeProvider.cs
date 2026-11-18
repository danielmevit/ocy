using System.Runtime.Versioning;
using Microsoft.Win32;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsTaskbarThemeProvider : ITaskbarThemeProvider
{
    private const string PersonalizeKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    private const string LightThemeValueName = "SystemUsesLightTheme";

    public bool IsSupported => OperatingSystem.IsWindows();

    public bool? IsLightTaskbar()
    {
        if (!IsSupported)
        {
            return null;
        }

        try
        {
            using RegistryKey? key = Registry.CurrentUser.OpenSubKey(PersonalizeKeyPath, writable: false);
            object? value = key?.GetValue(LightThemeValueName);
            return TaskbarThemeMath.FromRegistryValue(value);
        }
        catch
        {
            return null;
        }
    }
}
