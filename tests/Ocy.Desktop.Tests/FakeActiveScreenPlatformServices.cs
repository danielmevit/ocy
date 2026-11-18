using Ocy.Abstractions;
using Ocy.Core.Platform;

namespace Ocy.Desktop.Tests;

public sealed class FakeActiveScreenProvider : IActiveScreenProvider
{
    public bool IsSupported => true;
    public ScreenRect? ActiveBounds { get; set; }

    public FakeActiveScreenProvider(ScreenRect? activeBounds = null)
    {
        ActiveBounds = activeBounds;
    }

    public ScreenRect? GetActiveScreenBounds() => ActiveBounds;
}

public sealed class FakeWindowStyler : IWindowStyler
{
    public bool IsSupported => true;
    public List<nint> StyledHandles { get; } = [];

    public void MakeClickThrough(nint windowHandle)
    {
        StyledHandles.Add(windowHandle);
    }
}

public sealed class FakeActiveScreenPlatformServices : IPlatformServices
{
    public IUserIdleProvider Idle { get; } = UnsupportedPlatformServices.Instance.Idle;
    public IFocusStateProvider Focus { get; } = UnsupportedPlatformServices.Instance.Focus;
    public IAutostartService Autostart { get; } = UnsupportedPlatformServices.Instance.Autostart;
    public ISessionEventSource Session { get; } = UnsupportedPlatformServices.Instance.Session;

    public FakeActiveScreenProvider ActiveScreenProvider { get; }
    public FakeWindowStyler WindowStylerService { get; }

    public IActiveScreenProvider ActiveScreen => ActiveScreenProvider;
    public IWindowStyler WindowStyler => WindowStylerService;

    public FakeActiveScreenPlatformServices(ScreenRect? activeBounds = null)
    {
        ActiveScreenProvider = new FakeActiveScreenProvider(activeBounds);
        WindowStylerService = new FakeWindowStyler();
    }

    public void Dispose()
    {
    }
}
