using Ocy.Abstractions;

namespace Ocy.Core.Tests.TestDoubles;

public sealed class FakeSessionEventSource : ISessionEventSource
{
    public bool IsSupported { get; set; } = true;

    public event EventHandler<SessionEventKind>? SessionChanged;

    public void Raise(SessionEventKind kind)
    {
        SessionChanged?.Invoke(this, kind);
    }

    public void Start()
    {
    }

    public void Dispose()
    {
    }
}
