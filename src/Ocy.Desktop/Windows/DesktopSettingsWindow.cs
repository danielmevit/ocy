using Avalonia.Controls;
using Ocy.Desktop.Coordinating;
using Ocy.Desktop.Host;

namespace Ocy.Desktop.Windows;

public sealed class DesktopSettingsWindow : ISettingsWindow
{
    private readonly AppHost _host;
    private SettingsWindow? _currentWindow;

    public DesktopSettingsWindow(AppHost host)
    {
        _host = host ?? throw new ArgumentNullException(nameof(host));
    }

    public void Show()
    {
        if (_currentWindow == null)
        {
            _currentWindow = new SettingsWindow(_host);
            _currentWindow.Closed += (_, _) => _currentWindow = null;
            _currentWindow.Show();
        }
        else
        {
            if (_currentWindow.WindowState == WindowState.Minimized)
            {
                _currentWindow.WindowState = WindowState.Normal;
            }
            _currentWindow.Activate();
        }
    }
}
