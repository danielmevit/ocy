using System.Runtime.Versioning;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsPlatformServices : IPlatformServices
{
    public WindowsPlatformServices(string exePath)
    {
        Idle = new WindowsIdleProvider();
        Focus = new WindowsFocusStateProvider();
        Autostart = new WindowsAutostartService(exePath);
        Session = new WindowsSessionEventSource();
        ActiveScreen = new WindowsActiveScreenProvider();
        WindowStyler = new WindowsWindowStyler();
        Motion = new WindowsMotionPreference();
        TaskbarTheme = new WindowsTaskbarThemeProvider();
    }

    public IUserIdleProvider Idle { get; }

    public IFocusStateProvider Focus { get; }

    public IAutostartService Autostart { get; }

    public ISessionEventSource Session { get; }

    public IActiveScreenProvider ActiveScreen { get; }

    public IWindowStyler WindowStyler { get; }

    public IMotionPreference Motion { get; }

    public ITaskbarThemeProvider TaskbarTheme { get; }

    public void Dispose()
    {
        Session.Dispose();
    }
}
