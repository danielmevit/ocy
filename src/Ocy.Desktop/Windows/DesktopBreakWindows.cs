using Avalonia;
using Avalonia.Controls;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Platform;
using Ocy.Desktop.Coordinating;
using Ocy.Desktop.Layout;
using Ocy.Desktop.Localization;

namespace Ocy.Desktop.Windows;

public sealed class DesktopBreakWindows : IBreakWindows
{
    private readonly BreakEngine _engine;
    private readonly Strings _strings;
    private readonly Func<IReadOnlyList<ScreenInfo>>? _screenProvider;
    private readonly Action<Window> _showWindow;
    private readonly IPlatformServices _platformServices;
    private readonly List<Window> _activeWindows = [];
    private readonly List<ShortBreakWindow> _activeShortBreakWindows = [];
    private readonly List<EdgeFrameWindow> _activeEdgeFrameWindows = [];
    private WarningWindow? _activeWarningWindow;
    private LongBreakOverlayWindow? _activeLongBreakPrimaryWindow;

    public IReadOnlyList<Window> ActiveWindows => _activeWindows;

    public IReadOnlyList<EdgeFrameWindow> ActiveEdgeFrameWindows => _activeEdgeFrameWindows;

    public DesktopBreakWindows(
        BreakEngine engine,
        Strings strings,
        Func<IReadOnlyList<ScreenInfo>>? screenProvider = null,
        Action<Window>? showWindow = null,
        IPlatformServices? platformServices = null)
    {
        _engine = engine ?? throw new ArgumentNullException(nameof(engine));
        _strings = strings ?? throw new ArgumentNullException(nameof(strings));
        _screenProvider = screenProvider;
        _showWindow = showWindow ?? (w => w.Show());
        _platformServices = platformServices ?? UnsupportedPlatformServices.Instance;
    }

    private IReadOnlyList<ScreenInfo> QueryScreens(Window? window = null)
    {
        if (_screenProvider != null)
        {
            return _screenProvider();
        }

        return ScreenQuery.GetScreens(window);
    }

    public void ShowShortBreak(BreakInfo info)
    {
        CloseAll();

        var isFullscreen = _engine.Settings.ShortBreaks.Fullscreen;
        var screens = QueryScreens();
        if (screens.Count == 0)
        {
            return;
        }

        if (!isFullscreen)
        {
            var activeBounds = _platformServices.ActiveScreen.GetActiveScreenBounds();
            var selectedIndex = ActiveScreenSelector.Select(screens, activeBounds);
            var selectedScreen = screens[selectedIndex];

            var window = new ShortBreakWindow(
                _strings,
                () => _engine.SkipCurrentBreak(),
                isFullscreen: false,
                isInteractive: true,
                platformServices: _platformServices);
            window.UpdateExercise(info.ExerciseId);
            window.UpdateProgress(info.TotalDuration, info.TotalDuration);
            PositionCornerWindow(window, selectedScreen);
            _showWindow(window);

            _activeWindows.Add(window);
            _activeShortBreakWindows.Add(window);

            ShowEdgeFrames(screens, selectedIndex, EdgeFrameVariant.ShortBreak);
        }
        else
        {
            var activeBounds = _platformServices.ActiveScreen.GetActiveScreenBounds();
            var selectedIndex = ActiveScreenSelector.Select(screens, activeBounds);

            for (var i = 0; i < screens.Count; i++)
            {
                var screen = screens[i];
                if (i == selectedIndex)
                {
                    var window = new ShortBreakWindow(
                        _strings,
                        () => _engine.SkipCurrentBreak(),
                        isFullscreen: true,
                        isInteractive: true,
                        platformServices: _platformServices);

                    ConfigureFullscreenWindow(window, screen);
                    window.UpdateExercise(info.ExerciseId);
                    window.UpdateProgress(info.TotalDuration, info.TotalDuration);
                    _showWindow(window);

                    _activeWindows.Add(window);
                    _activeShortBreakWindows.Add(window);
                }
                else
                {
                    var dim = new DimOverlayWindow();
                    ConfigureFullscreenWindow(dim, screen);
                    _showWindow(dim);

                    _activeWindows.Add(dim);
                }
            }
        }
    }

    public void ShowWarning(TimeSpan countdown)
    {
        CloseAll();

        var window = new WarningWindow(
            _strings,
            onStartNow: () => _engine.StartLongBreakNow(),
            onPostpone: () => _engine.PostponeLongBreak(),
            onSkip: () => _engine.SkipCurrentBreak(),
            postponeMinutes: _engine.Settings.LongBreaks.PostponeMinutes);

        var screens = QueryScreens(window);
        if (screens.Count == 0)
        {
            return;
        }

        var activeBounds = _platformServices.ActiveScreen.GetActiveScreenBounds();
        var selectedIndex = ActiveScreenSelector.Select(screens, activeBounds);
        var selectedScreen = screens[selectedIndex];

        var snapshot = _engine.Snapshot;
        window.UpdateCountdown(countdown, snapshot.StrictMode, snapshot.PostponesLeft, _engine.Settings.LongBreaks.PostponeMinutes);
        PositionCornerWindow(window, selectedScreen);
        _showWindow(window);

        _activeWindows.Add(window);
        _activeWarningWindow = window;

        ShowEdgeFrames(screens, selectedIndex, EdgeFrameVariant.Warning);
    }

    public void ShowLongBreak(BreakInfo info)
    {
        CloseAll();

        var screens = QueryScreens();
        if (screens.Count == 0)
        {
            return;
        }

        var activeBounds = _platformServices.ActiveScreen.GetActiveScreenBounds();
        var selectedIndex = ActiveScreenSelector.Select(screens, activeBounds);

        var snapshot = _engine.Snapshot;

        for (var i = 0; i < screens.Count; i++)
        {
            var screen = screens[i];
            if (i == selectedIndex)
            {
                var window = new LongBreakOverlayWindow(
                    isPrimary: true,
                    info: info,
                    strings: _strings,
                    onSkip: () => _engine.SkipCurrentBreak(),
                    strictMode: snapshot.StrictMode,
                    platformServices: _platformServices);
                window.UpdateProgress(info.TotalDuration, info.TotalDuration);

                ConfigureFullscreenWindow(window, screen);
                _showWindow(window);

                _activeWindows.Add(window);
                _activeLongBreakPrimaryWindow = window;
            }
            else
            {
                var dim = new DimOverlayWindow();
                ConfigureFullscreenWindow(dim, screen);
                _showWindow(dim);

                _activeWindows.Add(dim);
            }
        }
    }

    private void ShowEdgeFrames(IReadOnlyList<ScreenInfo> screens, int selectedIndex, EdgeFrameVariant variant = EdgeFrameVariant.ShortBreak)
    {
        for (var i = 0; i < screens.Count; i++)
        {
            if (i == selectedIndex)
            {
                continue;
            }

            var screen = screens[i];
            var frameWindow = new EdgeFrameWindow(variant);
            ConfigureFullscreenWindow(frameWindow, screen);

            void ApplyClickThrough()
            {
                var handle = frameWindow.TryGetPlatformHandle()?.Handle;
                if (handle.HasValue && handle.Value != nint.Zero)
                {
                    _platformServices.WindowStyler.MakeClickThrough(handle.Value);
                }
            }

            frameWindow.Opened += (_, _) => ApplyClickThrough();
            _showWindow(frameWindow);
            ApplyClickThrough();

            _activeWindows.Add(frameWindow);
            _activeEdgeFrameWindows.Add(frameWindow);
        }
    }

    public void UpdateCountdown(EngineSnapshot snapshot)
    {
        if (_activeShortBreakWindows.Count > 0 && snapshot.CurrentBreak != null)
        {
            foreach (var w in _activeShortBreakWindows)
            {
                w.UpdateProgress(
                    snapshot.CurrentBreak.RemainingDuration,
                    snapshot.CurrentBreak.TotalDuration);
            }
        }
        else if (_activeWarningWindow != null && snapshot.TimeUntilLongBreak.HasValue)
        {
            _activeWarningWindow.UpdateCountdown(
                snapshot.TimeUntilLongBreak.Value,
                snapshot.StrictMode,
                snapshot.PostponesLeft,
                _engine.Settings.LongBreaks.PostponeMinutes);
        }
        else if (_activeLongBreakPrimaryWindow != null && snapshot.CurrentBreak != null)
        {
            _activeLongBreakPrimaryWindow.UpdateCountdown(
                snapshot.CurrentBreak.RemainingDuration,
                snapshot.StrictMode);
            _activeLongBreakPrimaryWindow.UpdateProgress(
                snapshot.CurrentBreak.RemainingDuration,
                snapshot.CurrentBreak.TotalDuration);
        }
    }

    public void CloseAll()
    {
        foreach (var w in _activeWindows)
        {
            try
            {
                if (w is ShortBreakWindow sb) sb.AllowClose = true;
                else if (w is WarningWindow ww) ww.AllowClose = true;
                else if (w is LongBreakOverlayWindow lb) lb.AllowClose = true;
                else if (w is DimOverlayWindow dim) dim.AllowClose = true;
                else if (w is EdgeFrameWindow ef) ef.AllowClose = true;

                w.Close();
            }
            catch
            {
                // Best effort close
            }
        }

        _activeWindows.Clear();
        _activeShortBreakWindows.Clear();
        _activeEdgeFrameWindows.Clear();
        _activeWarningWindow = null;
        _activeLongBreakPrimaryWindow = null;
    }

    private static void PositionCornerWindow(Window window, ScreenInfo screen)
    {
        var width = window.Width > 0 && !double.IsNaN(window.Width) ? window.Width : 392.0;
        var initialSize = new Size(width, 172.0);

        try
        {
            window.Measure(new Size(width, double.PositiveInfinity));
            var desired = window.DesiredSize;
            if (desired.Width > 0 && desired.Height > 0)
            {
                initialSize = desired;
            }
        }
        catch
        {
            // If font manager or layout engine is unavailable prior to window show, fall back to initialSize
        }

        window.Position = OverlayLayout.ComputeCornerPosition(screen, initialSize, 0.0);

        window.Opened += (_, _) =>
        {
            var realWidth = window.Bounds.Width > 0 ? window.Bounds.Width : window.DesiredSize.Width;
            var realHeight = window.Bounds.Height > 0 ? window.Bounds.Height : window.DesiredSize.Height;
            if (realWidth > 0 && realHeight > 0)
            {
                window.Position = OverlayLayout.ComputeCornerPosition(screen, new Size(realWidth, realHeight), 0.0);
            }
        };
    }

    private static void ConfigureFullscreenWindow(Window window, ScreenInfo screen)
    {
        var scaling = screen.Scaling > 0 ? screen.Scaling : 1.0;
        var initialDipSize = OverlayLayout.ComputeDipSize(screen.Bounds.Width, screen.Bounds.Height, scaling);
        window.Position = new PixelPoint(screen.Bounds.X, screen.Bounds.Y);
        window.Width = initialDipSize.Width;
        window.Height = initialDipSize.Height;

        window.Opened += (_, _) =>
        {
            window.Position = new PixelPoint(screen.Bounds.X, screen.Bounds.Y);
            var activeScaling = window.RenderScaling > 0 ? window.RenderScaling : scaling;
            var activeDipSize = OverlayLayout.ComputeDipSize(screen.Bounds.Width, screen.Bounds.Height, activeScaling);
            window.Width = activeDipSize.Width;
            window.Height = activeDipSize.Height;
        };
    }
}
