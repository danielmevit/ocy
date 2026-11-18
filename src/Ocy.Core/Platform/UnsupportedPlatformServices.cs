using Ocy.Abstractions;

namespace Ocy.Core.Platform;

/// <summary>
/// Fallback platform services implementation where all capabilities report unsupported or unknown,
/// and operations are no-ops. Used on unsupported platforms and in headless tests.
/// </summary>
public sealed class UnsupportedPlatformServices : IPlatformServices
{
    public static readonly UnsupportedPlatformServices Instance = new();

    public IUserIdleProvider Idle { get; } = new UnsupportedIdleProvider();

    public IFocusStateProvider Focus { get; } = new UnsupportedFocusStateProvider();

    public IAutostartService Autostart { get; } = new UnsupportedAutostartService();

    public ISessionEventSource Session { get; } = new UnsupportedSessionEventSource();

    public void Dispose()
    {
    }

    private sealed class UnsupportedIdleProvider : IUserIdleProvider
    {
        public bool IsSupported => false;

        public TimeSpan GetIdleTime() => TimeSpan.Zero;
    }

    private sealed class UnsupportedFocusStateProvider : IFocusStateProvider
    {
        public bool IsSupported => false;

        public FocusState GetState() => FocusState.Unknown;
    }

    private sealed class UnsupportedAutostartService : IAutostartService
    {
        public bool IsSupported => false;

        public bool IsEnabled() => false;

        public void SetEnabled(bool enabled)
        {
        }
    }

    private sealed class UnsupportedSessionEventSource : ISessionEventSource
    {
        public bool IsSupported => false;

        public event EventHandler<SessionEventKind>? SessionChanged
        {
            add { }
            remove { }
        }

        public void Start()
        {
        }

        public void Dispose()
        {
        }
    }
}
