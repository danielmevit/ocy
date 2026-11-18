using Ocy.Abstractions;

namespace Ocy.Core.Tests.TestDoubles;

public sealed class FakeIdleProvider : IUserIdleProvider
{
    public bool IsSupported { get; set; } = true;

    public TimeSpan IdleTime { get; set; } = TimeSpan.Zero;

    public TimeSpan GetIdleTime() => IsSupported ? IdleTime : TimeSpan.Zero;
}
