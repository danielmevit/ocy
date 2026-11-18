using Ocy.Abstractions;
using Ocy.Core.Platform;
using Xunit;

namespace Ocy.Core.Tests;

public sealed class UnsupportedPlatformServicesTests
{
    [Fact]
    public void UnsupportedPlatformServices_ReportsUnsupportedAndSafeDefaults()
    {
        var services = UnsupportedPlatformServices.Instance;

        Assert.False(services.Idle.IsSupported);
        Assert.Equal(TimeSpan.Zero, services.Idle.GetIdleTime());

        Assert.False(services.Focus.IsSupported);
        Assert.Equal(FocusState.Unknown, services.Focus.GetState());

        Assert.False(services.Autostart.IsSupported);
        Assert.False(services.Autostart.IsEnabled());
        // SetEnabled should not throw
        services.Autostart.SetEnabled(true);

        Assert.False(services.Session.IsSupported);
        services.Session.Start();
        services.Dispose();
    }
}
