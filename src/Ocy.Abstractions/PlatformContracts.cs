// FROZEN platform contracts — see docs/ARCHITECTURE.md §2.
// Lead-owned. Worker tasks implement these; they must not change them.

namespace Ocy.Abstractions;

/// <summary>Time since the user's last keyboard or mouse input.</summary>
public interface IUserIdleProvider
{
    /// <summary>False when this platform cannot report idle time (the engine then never enters Away by idle).</summary>
    bool IsSupported { get; }

    /// <summary>Idle time; <see cref="TimeSpan.Zero"/> when unsupported or on error. Never negative.</summary>
    TimeSpan GetIdleTime();
}

/// <summary>What the user is doing on screen, as far as breaks are concerned.</summary>
public enum FocusState
{
    Unknown = 0,
    Normal = 1,
    FullscreenApp = 2,
    Presentation = 3,
    QuietTime = 4,
    NotPresent = 5,
}

public interface IFocusStateProvider
{
    bool IsSupported { get; }

    /// <summary>Current state; <see cref="FocusState.Unknown"/> when unsupported or on error.</summary>
    FocusState GetState();
}

/// <summary>Start the app when the user signs in.</summary>
public interface IAutostartService
{
    bool IsSupported { get; }

    bool IsEnabled();

    /// <summary>Enable or disable autostart for the current user. Throws on failure; no-op when unsupported.</summary>
    void SetEnabled(bool enabled);
}

public enum SessionEventKind
{
    Locked = 1,
    Unlocked = 2,
    Suspending = 3,
    Resumed = 4,
    LoggingOff = 5,
}

/// <summary>OS session events. Raised on an arbitrary thread; consumers marshal to their own thread.</summary>
public interface ISessionEventSource : IDisposable
{
    bool IsSupported { get; }

    event EventHandler<SessionEventKind>? SessionChanged;

    /// <summary>Begin listening. Idempotent.</summary>
    void Start();
}

/// <summary>A rectangle in physical screen pixels (virtual-desktop coordinates; may be negative).</summary>
public readonly record struct ScreenRect(int X, int Y, int Width, int Height);

/// <summary>Which monitor the user is working on right now (added 0.1.1, FROZEN).</summary>
public interface IActiveScreenProvider
{
    bool IsSupported { get; }

    /// <summary>
    /// Full bounds (physical pixels) of the monitor the user is actively working on: the monitor of the
    /// foreground window, else the monitor under the mouse cursor. Null when unsupported or unknown.
    /// </summary>
    ScreenRect? GetActiveScreenBounds();
}

/// <summary>OS-level window styling the UI toolkit cannot express (added 0.1.1, FROZEN).</summary>
public interface IWindowStyler
{
    bool IsSupported { get; }

    /// <summary>Make a top-level window ignore mouse input and never activate (clicks pass through to what is below).
    /// No-op when unsupported. Never throws.</summary>
    void MakeClickThrough(nint windowHandle);
}

/// <summary>The user's OS-level animation preference (added 0.1.2, FROZEN).</summary>
public interface IMotionPreference
{
    bool IsSupported { get; }

    /// <summary>True when the OS asks apps to minimise animation (Windows: "Animation effects" off).
    /// False when animations are allowed or the preference is unknown. Never throws.</summary>
    bool PrefersReducedMotion();
}

/// <summary>The OS taskbar/notification-area colour theme (added 0.1.2, FROZEN). It can differ from the app theme on
/// Windows ("Choose your default Windows mode" vs "app mode").</summary>
public interface ITaskbarThemeProvider
{
    bool IsSupported { get; }

    /// <summary>True = light taskbar, false = dark taskbar, null = unknown. Never throws.</summary>
    bool? IsLightTaskbar();
}

/// <summary>Fallback used by the default interface members below.</summary>
public sealed class UnsupportedTaskbarThemeProvider : ITaskbarThemeProvider
{
    public static readonly UnsupportedTaskbarThemeProvider Instance = new();

    public bool IsSupported => false;

    public bool? IsLightTaskbar() => null;
}

/// <summary>Fallback used by the default interface members below.</summary>
public sealed class UnsupportedMotionPreference : IMotionPreference
{
    public static readonly UnsupportedMotionPreference Instance = new();

    public bool IsSupported => false;

    public bool PrefersReducedMotion() => false;
}

/// <summary>Fallback used by the default interface members below.</summary>
public sealed class UnsupportedActiveScreenProvider : IActiveScreenProvider
{
    public static readonly UnsupportedActiveScreenProvider Instance = new();

    public bool IsSupported => false;

    public ScreenRect? GetActiveScreenBounds() => null;
}

/// <summary>Fallback used by the default interface members below.</summary>
public sealed class UnsupportedWindowStyler : IWindowStyler
{
    public static readonly UnsupportedWindowStyler Instance = new();

    public bool IsSupported => false;

    public void MakeClickThrough(nint windowHandle)
    {
    }
}

/// <summary>The set of platform services the app composes at startup.</summary>
public interface IPlatformServices : IDisposable
{
    IUserIdleProvider Idle { get; }

    IFocusStateProvider Focus { get; }

    IAutostartService Autostart { get; }

    ISessionEventSource Session { get; }

    /// <summary>Added 0.1.1. Default: unsupported (implementations override).</summary>
    IActiveScreenProvider ActiveScreen => UnsupportedActiveScreenProvider.Instance;

    /// <summary>Added 0.1.1. Default: unsupported (implementations override).</summary>
    IWindowStyler WindowStyler => UnsupportedWindowStyler.Instance;

    /// <summary>Added 0.1.2. Default: unsupported → animations allowed (implementations override).</summary>
    IMotionPreference Motion => UnsupportedMotionPreference.Instance;

    /// <summary>Added 0.1.2. Default: unsupported → unknown (callers fall back to the app theme).</summary>
    ITaskbarThemeProvider TaskbarTheme => UnsupportedTaskbarThemeProvider.Instance;
}
