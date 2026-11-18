using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.Styling;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Exercises;
using Ocy.Core.Persistence;
using Ocy.Core.Platform;
using Ocy.Core.Settings;
using Ocy.Desktop.Host;
using Ocy.Desktop.Layout;
using Ocy.Desktop.Localization;
using Ocy.Desktop.Theme;
using Ocy.Desktop.Tray;
using Ocy.Desktop.Windows;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class HeadlessUiTests
{
    private readonly Strings _strings = new();

    [AvaloniaFact]
    public void ShortBreakWindow_ShowsTitleBodyAndX()
    {
        var skipped = false;
        var window = new ShortBreakWindow(_strings, onSkip: () => skipped = true, isFullscreen: false);

        window.UpdateExercise("blink");
        window.UpdateProgress(TimeSpan.FromSeconds(10), TimeSpan.FromSeconds(15));

        Assert.Equal(_strings.Get("exercise.blink.title"), window.TitleTextBlock.Text);
        Assert.Equal(_strings.Get("exercise.blink.body"), window.BodyTextBlock.Text);
        Assert.NotNull(window.SkipButtonControl);
        Assert.True(window.SkipButtonControl.IsVisible);

        // Progress bar value check
        Assert.True(window.ProgressBarControl.Value > 60.0 && window.ProgressBarControl.Value < 70.0);

        // Clicking skip invokes callback
        window.SkipButtonControl.RaiseEvent(new Avalonia.Interactivity.RoutedEventArgs(Avalonia.Controls.Button.ClickEvent));
        Assert.True(skipped);
    }

    [AvaloniaFact]
    public void WarningWindow_HidesPostponeAndSkipAppropriately()
    {
        var window = new WarningWindow(
            _strings,
            onStartNow: () => { },
            onPostpone: () => { },
            onSkip: () => { },
            postponeMinutes: 3);

        // 1. Normal mode with postpones left: both visible
        window.UpdateCountdown(TimeSpan.FromSeconds(45), strictMode: false, postponesLeft: 2, postponeMinutes: 3);
        Assert.True(window.PostponeButtonControl.IsVisible);
        Assert.True(window.SkipButtonControl.IsVisible);
        Assert.False(window.StrictMarkerTextBlock.IsVisible);
        Assert.Equal(_strings.Format("warning.title", ("time", "0:45")), window.TitleTextBlock.Text);

        // 2. Normal mode with 0 postpones left: postpone hidden, skip visible
        window.UpdateCountdown(TimeSpan.FromSeconds(30), strictMode: false, postponesLeft: 0, postponeMinutes: 3);
        Assert.False(window.PostponeButtonControl.IsVisible);
        Assert.True(window.SkipButtonControl.IsVisible);
        Assert.False(window.StrictMarkerTextBlock.IsVisible);

        // 3. Strict mode: both postpone and skip hidden, strict marker visible
        window.UpdateCountdown(TimeSpan.FromSeconds(15), strictMode: true, postponesLeft: 3, postponeMinutes: 3);
        Assert.False(window.PostponeButtonControl.IsVisible);
        Assert.False(window.SkipButtonControl.IsVisible);
        Assert.True(window.StrictMarkerTextBlock.IsVisible);
        Assert.Equal(_strings.Get("warning.strictOn"), window.StrictMarkerTextBlock.Text);
    }

    [AvaloniaFact]
    public void WarningWindow_TitleAndStrictMarker_UseAttentionBrush()
    {
        var window = new WarningWindow(
            _strings,
            onStartNow: () => { },
            onPostpone: () => { },
            onSkip: () => { },
            postponeMinutes: 3);
        window.Show();

        var app = Application.Current!;
        Assert.True(app.TryGetResource("AttentionBrush", app.ActualThemeVariant, out var attentionBrush));

        Assert.Equal(attentionBrush, window.TitleTextBlock.Foreground);
        Assert.Equal(attentionBrush, window.StrictMarkerTextBlock.Foreground);
        Assert.Equal(12, window.StrictMarkerTextBlock.FontSize);

        // Verify buttons are unchanged: Start now is accent green
        Assert.True(app.TryGetResource("AccentBrush", app.ActualThemeVariant, out var accentBrush));
        Assert.Contains("accent", window.StartNowButtonControl.Classes);
        Assert.Contains("outline", window.PostponeButtonControl.Classes);
        Assert.Contains("text", window.SkipButtonControl.Classes);

        window.AllowClose = true;
        window.Close();
    }

    [AvaloniaFact]
    public void EdgeFrameWindow_VariantAppliesCorrectBrush()
    {
        var shortBreakFrame = new EdgeFrameWindow(EdgeFrameVariant.ShortBreak);
        var warningFrame = new EdgeFrameWindow(EdgeFrameVariant.Warning);

        Assert.Equal(EdgeFrameVariant.ShortBreak, shortBreakFrame.Variant);
        Assert.Equal(EdgeFrameVariant.Warning, warningFrame.Variant);

        var app = Application.Current!;
        Assert.True(app.TryGetResource("EdgeFrameBrush", app.ActualThemeVariant, out var edgeBrush));
        Assert.True(app.TryGetResource("AttentionFrameBrush", app.ActualThemeVariant, out var attentionBrush));

        Assert.Equal(edgeBrush, shortBreakFrame.FrameBorderControl.BorderBrush);
        Assert.Equal(attentionBrush, warningFrame.FrameBorderControl.BorderBrush);
    }

    [AvaloniaFact]
    public void LongBreakOverlay_ShowsCountdownAndHidesSkipInStrictMode()
    {
        var skipped = false;
        var info = new BreakInfo(
            Kind: BreakKind.Long,
            ExerciseId: "neck-shoulders",
            TotalDuration: TimeSpan.FromMinutes(5),
            SuggestionExerciseIds: ["neck-shoulders", "blink"]);

        // 1. Non-strict overlay
        var window = new LongBreakOverlayWindow(
            isPrimary: true,
            info: info,
            strings: _strings,
            onSkip: () => skipped = true,
            strictMode: false,
            titleVariant: 1);

        Assert.Equal(_strings.Get("longbreak.title.1"), window.TitleTextBlock.Text);
        Assert.Equal(_strings.Get("longbreak.body.1"), window.BodyTextBlock.Text);
        Assert.Equal(_strings.Get("longbreak.countdownLabel"), window.CountdownLabelTextBlock.Text);
        Assert.Equal("5:00", window.CountdownTextBlock.Text);
        Assert.True(window.SkipButtonControl.IsVisible);
        Assert.Equal(_strings.Get("longbreak.skip"), window.SkipButtonControl.Content);
        Assert.True(window.SuggestionsPanelControl.IsVisible);
        Assert.Equal(_strings.Get("exercise.neck-shoulders.title"), window.Suggestion1TitleBlock.Text);
        Assert.Equal(_strings.Get("exercise.blink.title"), window.Suggestion2TitleBlock.Text);

        // Update countdown
        window.UpdateCountdown(TimeSpan.FromSeconds(45), strictMode: false);
        Assert.Equal("0:45", window.CountdownTextBlock.Text);
        Assert.True(window.SkipButtonControl.IsVisible);

        // Update to strict mode
        window.UpdateCountdown(TimeSpan.FromSeconds(30), strictMode: true);
        Assert.False(window.SkipButtonControl.IsVisible);
        Assert.Equal(2, Grid.GetColumnSpan(window.ProgressBarControl));

        // Skip action click test
        window.SkipButtonControl.RaiseEvent(new Avalonia.Interactivity.RoutedEventArgs(Avalonia.Controls.Button.ClickEvent));
        Assert.True(skipped);
    }

    [AvaloniaFact]
    public void LongBreakOverlay_CountdownIsNumeralsOnly_WithAccessibleSentence()
    {
        var info = new BreakInfo(BreakKind.Long, "blink", TimeSpan.FromMinutes(5), ["blink"]);
        var window = new LongBreakOverlayWindow(true, info, _strings, () => { }, strictMode: false, titleVariant: 2);

        foreach (var seconds in new[] { 300, 263, 59, 5, 0 })
        {
            window.UpdateCountdown(TimeSpan.FromSeconds(seconds), strictMode: false);
            var text = window.CountdownTextBlock.Text ?? string.Empty;
            Assert.Matches(@"^\d+:\d{2}$", text);
            Assert.Equal(
                _strings.Format("longbreak.countdown", ("time", text)),
                Avalonia.Automation.AutomationProperties.GetName(window.CountdownTextBlock));
        }

        Assert.Equal(FontWeight.Light, window.CountdownTextBlock.FontWeight);
        Assert.Equal(56, window.CountdownTextBlock.FontSize);
    }

    [AvaloniaFact]
    public void LongBreakOverlay_StrictMode_HidesSkip_AndProgressTakesFullWidth()
    {
        var info = new BreakInfo(BreakKind.Long, "blink", TimeSpan.FromMinutes(5), ["blink", "palming"]);
        var window = new LongBreakOverlayWindow(true, info, _strings, () => { }, strictMode: true, titleVariant: 3);
        window.Width = 1280;
        window.Height = 720;
        window.Show();

        Assert.False(window.SkipButtonControl.IsVisible);
        Assert.Equal(2, Grid.GetColumnSpan(window.ProgressBarControl));

        var card = window.CardBorderControl;
        var contentWidth = card.Bounds.Width - card.Padding.Left - card.Padding.Right;
        Assert.Equal(LongBreakOverlayWindow.CardWidth, card.Bounds.Width, 1);
        Assert.Equal(contentWidth, window.ProgressBarControl.Bounds.Width, 1);

        window.AllowClose = true;
        window.Close();
    }

    [AvaloniaFact]
    public void LongBreakOverlay_UpdateProgress_ShowsElapsedFraction()
    {
        var info = new BreakInfo(BreakKind.Long, "blink", TimeSpan.FromMinutes(5), ["blink"]);
        var window = new LongBreakOverlayWindow(true, info, _strings, () => { });

        Assert.Equal(0, window.ProgressBarControl.Value, 3);

        window.UpdateProgress(TimeSpan.FromSeconds(225), TimeSpan.FromMinutes(5));
        Assert.Equal(25, window.ProgressBarControl.Value, 3);

        window.UpdateProgress(TimeSpan.Zero, TimeSpan.FromMinutes(5));
        Assert.Equal(100, window.ProgressBarControl.Value, 3);

        // A zero total never divides by zero
        window.UpdateProgress(TimeSpan.Zero, TimeSpan.Zero);
        Assert.Equal(0, window.ProgressBarControl.Value, 3);
    }

    [AvaloniaFact]
    public void LongBreakOverlay_SuggestionsStack_OnNarrowScreens()
    {
        var info = new BreakInfo(BreakKind.Long, "blink", TimeSpan.FromMinutes(5), ["blink", "palming"]);
        var window = new LongBreakOverlayWindow(true, info, _strings, () => { });

        window.ApplyResponsiveLayout(1920);
        Assert.False(window.SuggestionsStacked);
        Assert.Equal(LongBreakOverlayWindow.CardWidth, window.CardBorderControl.Width);

        window.ApplyResponsiveLayout(440);
        Assert.True(window.SuggestionsStacked);
        Assert.Single(window.SuggestionsGridControl.ColumnDefinitions);
        Assert.True(window.CardBorderControl.Width <= 440 - 32);
    }

    [AvaloniaFact]
    public void DesktopBreakWindows_LongBreakTick_UpdatesCountdownAndProgress()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-lb-progress-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            var screen = new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true, workingArea: new PixelRect(0, 0, 1920, 1040));
            using var host = new AppHost(dataDir: tempDir);
            var breakWindows = new DesktopBreakWindows(host.Engine, _strings, () => [screen], showWindow: w => w.Show());

            var total = TimeSpan.FromMinutes(4);
            breakWindows.ShowLongBreak(new BreakInfo(BreakKind.Long, "blink", total, ["blink"]));
            var card = Assert.Single(breakWindows.ActiveWindows.OfType<LongBreakOverlayWindow>());
            Assert.Equal(0, card.ProgressBarControl.Value, 3);

            var snapshot = new EngineSnapshot(
                Phase: EnginePhase.LongBreak,
                TimeUntilShortBreak: null,
                TimeUntilLongBreak: null,
                CurrentBreak: new CurrentBreakInfo(BreakKind.Long, "blink", total, TimeSpan.FromMinutes(1)),
                PausedUntilUtc: null,
                PostponesLeft: 0,
                StrictMode: false);
            breakWindows.UpdateCountdown(snapshot);

            Assert.Equal("1:00", card.CountdownTextBlock.Text);
            Assert.Equal(75, card.ProgressBarControl.Value, 3);

            breakWindows.CloseAll();
        }
        finally
        {
            try
            {
                Directory.Delete(tempDir, recursive: true);
            }
            catch
            {
                // Best effort cleanup
            }
        }
    }

    [Fact]
    public void DesignTokens_OnAccentAndError_TokensMatchSpecification()
    {
        Assert.Equal(Color.Parse("#FFFFFF"), DesignTokens.LightOnAccent);
        Assert.Equal(Color.Parse("#0F1A15"), DesignTokens.DarkOnAccent);
        Assert.Equal(Color.Parse("#B3261E"), DesignTokens.LightError);
        Assert.Equal(Color.Parse("#FF8A80"), DesignTokens.DarkError);

        double Luminance(Color c)
        {
            double Srgb(double channel) => channel <= 0.03928 ? channel / 12.92 : Math.Pow((channel + 0.055) / 1.055, 2.4);
            return 0.2126 * Srgb(c.R / 255.0) + 0.7152 * Srgb(c.G / 255.0) + 0.0722 * Srgb(c.B / 255.0);
        }

        double Contrast(Color c1, Color c2)
        {
            var l1 = Luminance(c1);
            var l2 = Luminance(c2);
            var brighter = Math.Max(l1, l2);
            var darker = Math.Min(l1, l2);
            return (brighter + 0.05) / (darker + 0.05);
        }

        var darkContrast = Contrast(DesignTokens.DarkAccent, DesignTokens.DarkOnAccent);
        Assert.True(darkContrast >= 4.5, $"Dark theme contrast {darkContrast} must be >= 4.5:1");

        var lightContrast = Contrast(DesignTokens.LightAccent, DesignTokens.LightOnAccent);
        Assert.True(lightContrast >= 4.5, $"Light theme contrast {lightContrast} must be >= 4.5:1");
    }

    [AvaloniaFact]
    public void ShortBreak_Fullscreen_ShowsCardOnActiveScreen_AndDimOnOtherScreens()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-tests-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            using var host = new AppHost(dataDir: tempDir);
            host.Engine.UpdateSettings(new OcySettings
            {
                ShortBreaks = new ShortBreakSettings { Fullscreen = true }
            });

            var screens = new[]
            {
                new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true),
                new ScreenInfo(1920, 0, 1920, 1080, 1.0, isPrimary: false),
                new ScreenInfo(3840, 0, 1920, 1080, 1.0, isPrimary: false)
            };

            var breakWindows = new DesktopBreakWindows(host.Engine, _strings, () => screens, showWindow: _ => { });
            var info = new BreakInfo(BreakKind.Short, "blink", TimeSpan.FromSeconds(15));

            breakWindows.ShowShortBreak(info);

            Assert.Equal(3, breakWindows.ActiveWindows.Count);

            // Active screen (primary screen 0) gets ShortBreakWindow with card
            var sb = Assert.IsType<ShortBreakWindow>(breakWindows.ActiveWindows[0]);
            Assert.Equal(_strings.Get("exercise.blink.title"), sb.TitleTextBlock.Text);
            Assert.Equal(_strings.Get("exercise.blink.body"), sb.BodyTextBlock.Text);
            Assert.True(sb.SkipButtonControl.IsVisible);

            // Other screens get DimOverlayWindow without card
            Assert.IsType<DimOverlayWindow>(breakWindows.ActiveWindows[1]);
            Assert.IsType<DimOverlayWindow>(breakWindows.ActiveWindows[2]);

            // Updating countdown updates all windows
            breakWindows.UpdateCountdown(new EngineSnapshot(
                Phase: EnginePhase.ShortBreak,
                TimeUntilShortBreak: null,
                TimeUntilLongBreak: null,
                CurrentBreak: new CurrentBreakInfo(BreakKind.Short, "blink", TimeSpan.FromSeconds(15), TimeSpan.FromSeconds(10)),
                PausedUntilUtc: null,
                PostponesLeft: 3,
                StrictMode: false));

            Assert.True(sb.ProgressBarControl.Value > 60.0);

            breakWindows.CloseAll();
            Assert.Empty(breakWindows.ActiveWindows);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [AvaloniaFact]
    public void ScreenQuery_GetScreens_ReturnsValidScreens()
    {
        var screens = ScreenQuery.GetScreens();
        Assert.NotEmpty(screens);
        Assert.Single(screens, s => s.IsPrimary);
        foreach (var screen in screens)
        {
            Assert.True(screen.Bounds.Width > 0, "Screen width must be positive");
            Assert.True(screen.Bounds.Height > 0, "Screen height must be positive");
            Assert.True(screen.Scaling > 0, "Screen scaling must be positive");
        }
    }

    [AvaloniaFact]
    public void BreakWindows_PositionsCornerWindows_FromMeasuredRealSize()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-corner-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            using var host = new AppHost(dataDir: tempDir);
            var screen = new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true, workingArea: new PixelRect(0, 0, 1920, 1040));
            var breakWindows = new DesktopBreakWindows(host.Engine, _strings, () => [screen], showWindow: w => w.Show());

            breakWindows.ShowShortBreak(new BreakInfo(BreakKind.Short, "blink", TimeSpan.FromSeconds(15)));

            var window = Assert.Single(breakWindows.ActiveWindows);
            var expectedY = 1040 - (int)Math.Round(window.Bounds.Height > 0 ? window.Bounds.Height : window.DesiredSize.Height);
            Assert.Equal(expectedY, window.Position.Y);

            breakWindows.ShowWarning(TimeSpan.FromSeconds(30));
            var warningWindow = Assert.Single(breakWindows.ActiveWindows);
            var expectedWarningY = 1040 - (int)Math.Round(warningWindow.Bounds.Height > 0 ? warningWindow.Bounds.Height : warningWindow.DesiredSize.Height);
            Assert.Equal(expectedWarningY, warningWindow.Position.Y);

            breakWindows.CloseAll();
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [AvaloniaFact]
    public void ShortBreak_ActiveScreen_PlacesCardOnActiveScreen_AndEdgeFramesOnOtherScreensOnly()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-active-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            var screens = new[]
            {
                new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true, workingArea: new PixelRect(0, 0, 1920, 1040)),
                new ScreenInfo(-3840, 0, 1920, 1200, 1.0, isPrimary: false, workingArea: new PixelRect(-3840, 0, 1920, 1200)),
                new ScreenInfo(-1920, 0, 1920, 1200, 1.0, isPrimary: false, workingArea: new PixelRect(-1920, 0, 1920, 1200))
            };

            // Active screen is secondary screen 2 (-1920, 0, 1920, 1200)
            var fakePlatform = new FakeActiveScreenPlatformServices(new ScreenRect(-1920, 0, 1920, 1200));
            using var host = new AppHost(dataDir: tempDir, platformServices: fakePlatform);

            var breakWindows = new DesktopBreakWindows(
                host.Engine,
                _strings,
                screenProvider: () => screens,
                showWindow: w => w.Show(),
                platformServices: fakePlatform);

            var info = new BreakInfo(BreakKind.Short, "blink", TimeSpan.FromSeconds(15));
            breakWindows.ShowShortBreak(info);

            // Card window should be on secondary screen 2
            var shortBreakWindows = breakWindows.ActiveWindows.OfType<ShortBreakWindow>().ToList();
            var cardWindow = Assert.Single(shortBreakWindows);
            Assert.True(cardWindow.Position.X >= -1920 && cardWindow.Position.X < 0,
                $"Card window Position.X was {cardWindow.Position.X}, expected on screen 2 [-1920, 0)");

            // Edge frames must be created for screen 0 and screen 1 only (1 frame each = 2 total)
            var frameWindows = breakWindows.ActiveEdgeFrameWindows;
            Assert.Equal(2, frameWindows.Count);
            Assert.Equal(3, breakWindows.ActiveWindows.Count);

            foreach (var frame in frameWindows)
            {
                Assert.True(frame.Topmost);
                Assert.False(frame.ShowActivated);
                Assert.False(frame.ShowInTaskbar);
                Assert.False(frame.CanResize);
                Assert.Equal(WindowDecorations.None, frame.WindowDecorations);
                Assert.Equal(WindowTransparencyLevel.Transparent, frame.TransparencyLevelHint[0]);
                Assert.Equal(3.0, frame.FrameBorderControl.BorderThickness.Left);
                Assert.Equal(EdgeFrameVariant.ShortBreak, frame.Variant);
                if (Application.Current != null && Application.Current.TryGetResource("EdgeFrameBrush", Application.Current.ActualThemeVariant, out var greenBrush))
                {
                    Assert.Equal(greenBrush, frame.FrameBorderControl.BorderBrush);
                }

                // No frame should be on screen 2
                var onScreen2 = frame.Position.X >= -1920 && frame.Position.X < 0;
                Assert.False(onScreen2, $"Edge frame at {frame.Position} must not be on active screen 2");
            }

            // CloseAll cleans up everything
            breakWindows.CloseAll();
            Assert.Empty(breakWindows.ActiveWindows);
            Assert.Empty(breakWindows.ActiveEdgeFrameWindows);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [AvaloniaFact]
    public void Warning_ActiveScreen_PlacesCardOnActiveScreen_AndEdgeFramesOnOtherScreensOnly()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-warning-active-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            var screens = new[]
            {
                new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true, workingArea: new PixelRect(0, 0, 1920, 1040)),
                new ScreenInfo(-3840, 0, 1920, 1200, 1.0, isPrimary: false, workingArea: new PixelRect(-3840, 0, 1920, 1200)),
                new ScreenInfo(-1920, 0, 1920, 1200, 1.0, isPrimary: false, workingArea: new PixelRect(-1920, 0, 1920, 1200))
            };

            // Active screen is secondary screen 1 (-3840, 0, 1920, 1200)
            var fakePlatform = new FakeActiveScreenPlatformServices(new ScreenRect(-3840, 0, 1920, 1200));
            using var host = new AppHost(dataDir: tempDir, platformServices: fakePlatform);

            var breakWindows = new DesktopBreakWindows(
                host.Engine,
                _strings,
                screenProvider: () => screens,
                showWindow: w => w.Show(),
                platformServices: fakePlatform);

            breakWindows.ShowWarning(TimeSpan.FromSeconds(30));

            // Warning window should be on secondary screen 1
            var warningWindows = breakWindows.ActiveWindows.OfType<WarningWindow>().ToList();
            var warningWindow = Assert.Single(warningWindows);
            Assert.True(warningWindow.Position.X >= -3840 && warningWindow.Position.X < -1920,
                $"Warning window Position.X was {warningWindow.Position.X}, expected on screen 1 [-3840, -1920)");

            // Edge frames must be created for screen 0 and screen 2 only (1 frame each = 2 total)
            var frameWindows = breakWindows.ActiveEdgeFrameWindows;
            Assert.Equal(2, frameWindows.Count);
            Assert.Equal(3, breakWindows.ActiveWindows.Count);

            foreach (var frame in frameWindows)
            {
                Assert.Equal(EdgeFrameVariant.Warning, frame.Variant);
                if (Application.Current != null && Application.Current.TryGetResource("AttentionFrameBrush", Application.Current.ActualThemeVariant, out var attentionBrush))
                {
                    Assert.Equal(attentionBrush, frame.FrameBorderControl.BorderBrush);
                }

                var onScreen1 = frame.Position.X >= -3840 && frame.Position.X < -1920;
                Assert.False(onScreen1, $"Edge frame at {frame.Position} must not be on active screen 1");
            }

            breakWindows.CloseAll();
            Assert.Empty(breakWindows.ActiveWindows);
            Assert.Empty(breakWindows.ActiveEdgeFrameWindows);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [AvaloniaFact]
    public void LongBreak_ActiveScreen_PlacesCardOnActiveScreen_AndDimsOtherScreens()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-long-active-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            var screens = new[]
            {
                new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true, workingArea: new PixelRect(0, 0, 1920, 1040)),
                new ScreenInfo(-3840, 0, 1920, 1200, 1.0, isPrimary: false, workingArea: new PixelRect(-3840, 0, 1920, 1200)),
                new ScreenInfo(-1920, 0, 1920, 1200, 1.0, isPrimary: false, workingArea: new PixelRect(-1920, 0, 1920, 1200))
            };

            // Active screen is secondary screen 2 (-1920, 0, 1920, 1200)
            var fakePlatform = new FakeActiveScreenPlatformServices(new ScreenRect(-1920, 0, 1920, 1200));
            using var host = new AppHost(dataDir: tempDir, platformServices: fakePlatform);

            var breakWindows = new DesktopBreakWindows(
                host.Engine,
                _strings,
                screenProvider: () => screens,
                showWindow: w => w.Show(),
                platformServices: fakePlatform);

            var info = new BreakInfo(
                BreakKind.Long,
                "neck-shoulders",
                TimeSpan.FromMinutes(5),
                SuggestionExerciseIds: ["neck-shoulders", "blink"]);

            breakWindows.ShowLongBreak(info);

            Assert.Equal(3, breakWindows.ActiveWindows.Count);

            // Active screen 2 gets the card window
            var cardWindows = breakWindows.ActiveWindows.OfType<LongBreakOverlayWindow>().ToList();
            var cardWindow = Assert.Single(cardWindows);
            Assert.True(cardWindow.Position.X >= -1920 && cardWindow.Position.X < 0,
                $"Card overlay Position.X was {cardWindow.Position.X}, expected on screen 2 [-1920, 0)");
            Assert.True(cardWindow.CardBorderControl.IsVisible);

            // Other screens (0 and 1) get dim overlays without cards
            var dimWindows = breakWindows.ActiveWindows.OfType<DimOverlayWindow>().ToList();
            Assert.Equal(2, dimWindows.Count);

            breakWindows.CloseAll();
            Assert.Empty(breakWindows.ActiveWindows);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [AvaloniaFact]
    public void BreakWindows_CloseGuard_CancelsDirectClose_UntilAllowCloseSet()
    {
        // 1. ShortBreakWindow
        var sb = new ShortBreakWindow(_strings, () => { });
        sb.Show();
        sb.Close();
        Assert.True(sb.IsVisible);
        sb.AllowClose = true;
        sb.Close();
        Assert.False(sb.IsVisible);

        // 2. WarningWindow
        var warn = new WarningWindow(_strings, () => { }, () => { }, () => { });
        warn.Show();
        warn.Close();
        Assert.True(warn.IsVisible);
        warn.AllowClose = true;
        warn.Close();
        Assert.False(warn.IsVisible);

        // 3. LongBreakOverlayWindow
        var lb = new LongBreakOverlayWindow(isPrimary: true, null, _strings, () => { });
        lb.Show();
        lb.Close();
        Assert.True(lb.IsVisible);
        lb.AllowClose = true;
        lb.Close();
        Assert.False(lb.IsVisible);

        // 4. DimOverlayWindow
        var dim = new DimOverlayWindow();
        dim.Show();
        dim.Close();
        Assert.True(dim.IsVisible);
        dim.AllowClose = true;
        dim.Close();
        Assert.False(dim.IsVisible);

        // 5. EdgeFrameWindow
        var frame = new EdgeFrameWindow();
        frame.Show();
        frame.Close();
        Assert.True(frame.IsVisible);
        frame.AllowClose = true;
        frame.Close();
        Assert.False(frame.IsVisible);
    }

    [AvaloniaFact]
    public void DesktopBreakWindows_CloseAll_ClosesGuardedWindows()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-closeguard-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            var screens = new[]
            {
                new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true, workingArea: new PixelRect(0, 0, 1920, 1040)),
                new ScreenInfo(-1920, 0, 1920, 1080, 1.0, isPrimary: false, workingArea: new PixelRect(-1920, 0, 1920, 1080))
            };

            var fakePlatform = new FakeActiveScreenPlatformServices(new ScreenRect(0, 0, 1920, 1080));
            using var host = new AppHost(dataDir: tempDir, platformServices: fakePlatform);

            var breakWindows = new DesktopBreakWindows(
                host.Engine,
                _strings,
                screenProvider: () => screens,
                showWindow: w => w.Show(),
                platformServices: fakePlatform);

            breakWindows.ShowShortBreak(new BreakInfo(BreakKind.Short, "blink", TimeSpan.FromSeconds(15)));
            Assert.Equal(2, breakWindows.ActiveWindows.Count); // 1 card + 1 edge frame

            // Direct close attempts are cancelled by the guard
            foreach (var win in breakWindows.ActiveWindows)
            {
                win.Close();
                Assert.True(win.IsVisible);
            }

            // CloseAll overrides guard and closes everything
            breakWindows.CloseAll();
            Assert.Empty(breakWindows.ActiveWindows);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [AvaloniaFact]
    public void ShortBreak_Fullscreen_ActiveScreen_PlacesCardOnActiveScreen_AndDimOnOtherScreens()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-fs-active-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            var screens = new[]
            {
                new ScreenInfo(0, 0, 1920, 1080, 1.0, isPrimary: true, workingArea: new PixelRect(0, 0, 1920, 1040)),
                new ScreenInfo(-3840, 0, 1920, 1200, 1.0, isPrimary: false, workingArea: new PixelRect(-3840, 0, 1920, 1200)),
                new ScreenInfo(-1920, 0, 1920, 1200, 1.0, isPrimary: false, workingArea: new PixelRect(-1920, 0, 1920, 1200))
            };

            // Active screen is secondary screen 2 (-1920, 0, 1920, 1200)
            var fakePlatform = new FakeActiveScreenPlatformServices(new ScreenRect(-1920, 0, 1920, 1200));
            using var host = new AppHost(dataDir: tempDir, platformServices: fakePlatform);
            host.Engine.UpdateSettings(new OcySettings
            {
                ShortBreaks = new ShortBreakSettings { Fullscreen = true }
            });

            var breakWindows = new DesktopBreakWindows(
                host.Engine,
                _strings,
                screenProvider: () => screens,
                showWindow: w => w.Show(),
                platformServices: fakePlatform);

            var info = new BreakInfo(BreakKind.Short, "blink", TimeSpan.FromSeconds(15));
            breakWindows.ShowShortBreak(info);

            Assert.Equal(3, breakWindows.ActiveWindows.Count);

            // Card window should be on secondary screen 2 (index 2)
            var shortBreakWindows = breakWindows.ActiveWindows.OfType<ShortBreakWindow>().ToList();
            var cardWindow = Assert.Single(shortBreakWindows);
            Assert.True(cardWindow.Position.X >= -1920 && cardWindow.Position.X < 0,
                $"Fullscreen short break card Position.X was {cardWindow.Position.X}, expected on active screen 2");
            Assert.True(cardWindow.CardBorderControl.IsVisible);

            // Other screens (0 and 1) get DimOverlayWindow without a card
            var dimWindows = breakWindows.ActiveWindows.OfType<DimOverlayWindow>().ToList();
            Assert.Equal(2, dimWindows.Count);

            breakWindows.CloseAll();
            Assert.Empty(breakWindows.ActiveWindows);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [AvaloniaFact]
    public void TrayIconRenderer_EveryState_ResolvesToLoadableCachedIcon()
    {
        var active = TrayIconRenderer.GetIcon(TrayIconState.Active);
        var paused = TrayIconRenderer.GetIcon(TrayIconState.Paused);
        var onBreak = TrayIconRenderer.GetIcon(TrayIconState.OnBreak);
        var attention = TrayIconRenderer.GetIcon(TrayIconState.Attention);

        Assert.NotNull(active);
        Assert.NotNull(paused);
        Assert.NotNull(onBreak);
        Assert.NotNull(attention);

        // Cached instances
        Assert.Same(active, TrayIconRenderer.GetIcon(TrayIconState.Active));
        Assert.Same(paused, TrayIconRenderer.GetIcon(TrayIconState.Paused));
        Assert.Same(onBreak, TrayIconRenderer.GetIcon(TrayIconState.OnBreak));
        Assert.Same(attention, TrayIconRenderer.GetIcon(TrayIconState.Attention));

        // Settings window and break windows carry the app icon
        using var host = new AppHost(dataDir: Path.Combine(Path.GetTempPath(), "ocy-icon-" + Guid.NewGuid().ToString("N")));
        var settingsWindow = new SettingsWindow(host);
        Assert.NotNull(settingsWindow.Icon);
        Assert.Same(AppIcon.GetIcon(), settingsWindow.Icon);

        var shortBreakWindow = new ShortBreakWindow(_strings, () => { });
        Assert.NotNull(shortBreakWindow.Icon);
        Assert.Same(AppIcon.GetIcon(), shortBreakWindow.Icon);

        var warningWindow = new WarningWindow(_strings, () => { }, () => { }, () => { });
        Assert.NotNull(warningWindow.Icon);
        Assert.Same(AppIcon.GetIcon(), warningWindow.Icon);

        var longBreakWindow = new LongBreakOverlayWindow(true, null, _strings, () => { });
        Assert.NotNull(longBreakWindow.Icon);
        Assert.Same(AppIcon.GetIcon(), longBreakWindow.Icon);

        var dimWindow = new DimOverlayWindow();
        Assert.NotNull(dimWindow.Icon);
        Assert.Same(AppIcon.GetIcon(), dimWindow.Icon);

        var edgeFrameWindow = new EdgeFrameWindow();
        Assert.NotNull(edgeFrameWindow.Icon);
        Assert.Same(AppIcon.GetIcon(), edgeFrameWindow.Icon);
    }

    [AvaloniaFact]
    public void LongBreakOverlayWindow_Keyboard_EscSkipsWhenNotStrict_AndIgnoredWhenStrict()
    {
        var nonStrictSkipped = false;
        var nonStrict = new LongBreakOverlayWindow(
            isPrimary: true,
            info: null,
            strings: _strings,
            onSkip: () => nonStrictSkipped = true,
            strictMode: false);
        nonStrict.Show();

        // Esc triggers skip when not strict
        nonStrict.RaiseEvent(new KeyEventArgs
        {
            RoutedEvent = InputElement.KeyDownEvent,
            Key = Key.Escape
        });
        Assert.True(nonStrictSkipped);
        nonStrict.AllowClose = true;
        nonStrict.Close();

        var strictSkipped = false;
        var strict = new LongBreakOverlayWindow(
            isPrimary: true,
            info: null,
            strings: _strings,
            onSkip: () => strictSkipped = true,
            strictMode: true);
        strict.Show();

        // Esc does not trigger skip when strict
        strict.RaiseEvent(new KeyEventArgs
        {
            RoutedEvent = InputElement.KeyDownEvent,
            Key = Key.Escape
        });
        Assert.False(strictSkipped);
        strict.AllowClose = true;
        strict.Close();
    }
}
