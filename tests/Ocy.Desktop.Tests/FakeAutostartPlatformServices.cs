using Ocy.Abstractions;
using Ocy.Core.Platform;

namespace Ocy.Desktop.Tests;

public sealed class FakeAutostartService : IAutostartService
{
    public bool IsSupported => true;
    public bool Enabled { get; set; } = true;
    public bool ThrowOnSet { get; set; }

    public bool IsEnabled() => Enabled;

    public void SetEnabled(bool enabled)
    {
        if (ThrowOnSet)
        {
            throw new InvalidOperationException("Autostart change failed (test).");
        }

        Enabled = enabled;
    }
}

public sealed class FakeAutostartPlatformServices : IPlatformServices
{
    public IUserIdleProvider Idle { get; } = UnsupportedPlatformServices.Instance.Idle;
    public IFocusStateProvider Focus { get; } = UnsupportedPlatformServices.Instance.Focus;
    public IAutostartService Autostart { get; } = new FakeAutostartService();
    public ISessionEventSource Session { get; } = UnsupportedPlatformServices.Instance.Session;

    public void Dispose()
    {
    }
}
