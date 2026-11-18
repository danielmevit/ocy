using Ocy.Abstractions;

namespace Ocy.Core.Tests.TestDoubles;

public sealed class TestPlatformServices : IPlatformServices
{
    public FakeIdleProvider Idle { get; set; } = new();

    public FakeFocusStateProvider Focus { get; set; } = new();

    public FakeSessionEventSource Session { get; set; } = new();

    public IAutostartService Autostart { get; set; } = new StubAutostartService();

    IUserIdleProvider IPlatformServices.Idle => Idle;

    IFocusStateProvider IPlatformServices.Focus => Focus;

    ISessionEventSource IPlatformServices.Session => Session;

    public void Dispose()
    {
        Session.Dispose();
    }

    private sealed class StubAutostartService : IAutostartService
    {
        public bool IsSupported => false;

        public bool IsEnabled() => false;

        public void SetEnabled(bool enabled)
        {
        }
    }
}
