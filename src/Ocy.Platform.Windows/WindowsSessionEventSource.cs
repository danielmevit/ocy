using System.Runtime.Versioning;
using Microsoft.Win32;
using Ocy.Abstractions;

namespace Ocy.Platform.Windows;

[SupportedOSPlatform("windows")]
public sealed class WindowsSessionEventSource : ISessionEventSource
{
    private readonly object _gate = new();
    private bool _started;
    private bool _disposed;

    public bool IsSupported => OperatingSystem.IsWindows();

    public event EventHandler<SessionEventKind>? SessionChanged;

    public void Start()
    {
        if (!IsSupported)
        {
            return;
        }

        lock (_gate)
        {
            if (_started || _disposed)
            {
                return;
            }

            _started = true;
            SystemEvents.SessionSwitch += OnSessionSwitch;
            SystemEvents.PowerModeChanged += OnPowerModeChanged;
        }
    }

    public void Dispose()
    {
        lock (_gate)
        {
            if (_disposed)
            {
                return;
            }

            _disposed = true;
            if (_started)
            {
                SystemEvents.SessionSwitch -= OnSessionSwitch;
                SystemEvents.PowerModeChanged -= OnPowerModeChanged;
            }
        }
    }

    private void OnSessionSwitch(object? sender, SessionSwitchEventArgs e)
    {
        Raise(SessionEventMapper.FromSessionSwitch((int)e.Reason));
    }

    private void OnPowerModeChanged(object? sender, PowerModeChangedEventArgs e)
    {
        Raise(SessionEventMapper.FromPowerMode((int)e.Mode));
    }

    private void Raise(SessionEventKind? kind)
    {
        if (kind is null)
        {
            return;
        }

        SessionChanged?.Invoke(this, kind.Value);
    }
}
