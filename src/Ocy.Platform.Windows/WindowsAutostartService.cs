using System.Runtime.Versioning;
using Microsoft.Win32;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsAutostartService : IAutostartService
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string DefaultValueName = "Ocy";

    private readonly string _command;
    private readonly string _valueName;

    public WindowsAutostartService(string exePath)
        : this(exePath, DefaultValueName)
    {
    }

    public WindowsAutostartService(string exePath, string valueName)
    {
        _command = AutostartCommand.Build(exePath);
        _valueName = valueName;
    }

    public bool IsSupported => OperatingSystem.IsWindows();

    public bool IsEnabled()
    {
        if (!IsSupported)
        {
            return false;
        }

        using RegistryKey? key = Registry.CurrentUser.OpenSubKey(RunKeyPath);
        if (key is null)
        {
            return false;
        }

        object? value = key.GetValue(_valueName);
        return value is string text && string.Equals(text, _command, StringComparison.OrdinalIgnoreCase);
    }

    public void SetEnabled(bool enabled)
    {
        if (!IsSupported)
        {
            return;
        }

        if (enabled)
        {
            using RegistryKey key = Registry.CurrentUser.CreateSubKey(RunKeyPath, writable: true);
            key.SetValue(_valueName, _command, RegistryValueKind.String);
        }
        else
        {
            using RegistryKey? key = Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true);
            if (key is null)
            {
                return;
            }

            key.DeleteValue(_valueName, throwOnMissingValue: false);
        }
    }
}
