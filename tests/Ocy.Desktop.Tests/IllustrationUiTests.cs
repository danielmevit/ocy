using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Styling;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Platform;
using Ocy.Desktop.Illustrations;
using Ocy.Desktop.Localization;
using Ocy.Desktop.Windows;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class FakeMotionPreference : IMotionPreference
{
    public bool IsSupported => true;
    public bool ReducedMotion { get; set; }

    public FakeMotionPreference(bool reducedMotion = false)
    {
        ReducedMotion = reducedMotion;
    }

    public bool PrefersReducedMotion() => ReducedMotion;
}

public sealed class FakeMotionPlatformServices : IPlatformServices
{
    public IUserIdleProvider Idle => UnsupportedPlatformServices.Instance.Idle;
    public IFocusStateProvider Focus => UnsupportedPlatformServices.Instance.Focus;
    public IAutostartService Autostart => UnsupportedPlatformServices.Instance.Autostart;
    public ISessionEventSource Session => UnsupportedPlatformServices.Instance.Session;
    public IMotionPreference Motion { get; }

    public FakeMotionPlatformServices(bool reducedMotion = true)
    {
        Motion = new FakeMotionPreference(reducedMotion);
    }

    public void Dispose()
    {
    }
}

public sealed class IllustrationUiTests
{
    private readonly Strings _strings = new();

    [AvaloniaFact]
    public void ShortBreak_ShowsIllustrationForBlink_AndNoneForIdWithoutArt()
    {
        var window = new ShortBreakWindow(_strings, () => { }, isFullscreen: false);
        window.Show();

        // 1. Exercise with art: "blink"
        window.UpdateExercise("blink");
        Assert.True(window.IllustrationControl.IsVisible);
        Assert.Equal(56, window.IllustrationControl.Size);
        Assert.Equal("blink", window.IllustrationControl.ExerciseId);
        Assert.NotNull(window.IllustrationControl.CurrentDrawing);

        // 2. Exercise id without art in the manifest
        window.UpdateExercise("test-no-art");
        Assert.False(window.IllustrationControl.IsVisible);
        Assert.Null(window.IllustrationControl.CurrentDrawing);

        // 3. Another exercise with art: "stretch-arms"
        window.UpdateExercise("stretch-arms");
        Assert.True(window.IllustrationControl.IsVisible);
        Assert.Equal("stretch-arms", window.IllustrationControl.ExerciseId);
        Assert.NotNull(window.IllustrationControl.CurrentDrawing);

        // 4. Null exercise collapses illustration
        window.UpdateExercise(null);
        Assert.False(window.IllustrationControl.IsVisible);

        window.AllowClose = true;
        window.Close();
    }

    [AvaloniaFact]
    public void ReducedMotion_ShowsFrozenFrame_AndRunsNoTimer()
    {
        var fakePlatform = new FakeMotionPlatformServices(reducedMotion: true);
        var window = new ShortBreakWindow(
            _strings,
            () => { },
            isFullscreen: false,
            isInteractive: true,
            platformServices: fakePlatform);

        window.Show();

        // Stretch arms has frozen = 3 in manifest
        window.UpdateExercise("stretch-arms");
        IllustrationManifest.Instance.TryGet("stretch-arms", out var info);
        Assert.NotNull(info);
        Assert.Equal(3, info.Frozen);

        Assert.True(window.IllustrationControl.IsVisible);
        Assert.Equal(info.Frozen, window.IllustrationControl.CurrentFrame);
        Assert.False(window.IllustrationControl.IsTimerRunning);

        // Blink has frozen = 1 in manifest
        window.UpdateExercise("blink");
        IllustrationManifest.Instance.TryGet("blink", out var blinkInfo);
        Assert.NotNull(blinkInfo);
        Assert.Equal(1, blinkInfo.Frozen);

        Assert.Equal(blinkInfo.Frozen, window.IllustrationControl.CurrentFrame);
        Assert.False(window.IllustrationControl.IsTimerRunning);

        window.AllowClose = true;
        window.Close();
    }

    [AvaloniaFact]
    public void ThemeSwitch_UpdatesToDarkKeys()
    {
        var window = new ShortBreakWindow(_strings, () => { }, isFullscreen: false);
        window.Show();
        window.UpdateExercise("blink");

        // Light mode
        window.RequestedThemeVariant = ThemeVariant.Light;
        window.IllustrationControl.UpdateDrawing();
        Assert.NotNull(window.IllustrationControl.CurrentDrawing);
        var lightDrawing = window.IllustrationControl.CurrentDrawing;

        Assert.True(window.TryFindResource("Ill.blink.light.f1", out var lightRes));
        Assert.Same(lightRes, lightDrawing);

        // Switch to Dark mode
        window.RequestedThemeVariant = ThemeVariant.Dark;
        window.IllustrationControl.UpdateDrawing();
        Assert.NotNull(window.IllustrationControl.CurrentDrawing);
        var darkDrawing = window.IllustrationControl.CurrentDrawing;

        // Verify that dark drawing matches the dark resource
        Assert.True(window.TryFindResource("Ill.blink.dark.f1", out var darkRes));
        Assert.NotNull(darkRes);
        Assert.Same(darkRes, darkDrawing);
        Assert.NotSame(lightDrawing, darkDrawing);

        window.AllowClose = true;
        window.Close();
    }

    [AvaloniaFact]
    public void LongBreakSuggestions_Show40PxIllustrations_WhenAvailable()
    {
        // 1. Both suggestions have art (stretch-arms and blink)
        var infoWithArt = new BreakInfo(
            Kind: BreakKind.Long,
            ExerciseId: "stretch-arms",
            TotalDuration: TimeSpan.FromMinutes(5),
            SuggestionExerciseIds: ["stretch-arms", "blink"]);

        var windowWithArt = new LongBreakOverlayWindow(
            isPrimary: true,
            info: infoWithArt,
            strings: _strings,
            onSkip: () => { });

        windowWithArt.Show();

        // Suggestion 1: stretch-arms (has art)
        Assert.NotNull(windowWithArt.Suggestion1IllustrationControl);
        Assert.True(windowWithArt.Suggestion1IllustrationControl.IsVisible);
        Assert.Equal(40, windowWithArt.Suggestion1IllustrationControl.Size);
        Assert.Equal("stretch-arms", windowWithArt.Suggestion1IllustrationControl.ExerciseId);
        Assert.NotNull(windowWithArt.Suggestion1IllustrationControl.CurrentDrawing);

        // Suggestion 2: blink (has art)
        Assert.NotNull(windowWithArt.Suggestion2IllustrationControl);
        Assert.True(windowWithArt.Suggestion2IllustrationControl.IsVisible);
        Assert.Equal(40, windowWithArt.Suggestion2IllustrationControl.Size);
        Assert.Equal("blink", windowWithArt.Suggestion2IllustrationControl.ExerciseId);
        Assert.NotNull(windowWithArt.Suggestion2IllustrationControl.CurrentDrawing);

        // Responsive stacked mode still shows both 40 px illustrations
        windowWithArt.ApplyResponsiveLayout(380);
        Assert.True(windowWithArt.SuggestionsStacked);
        Assert.True(windowWithArt.Suggestion1IllustrationControl.IsVisible);
        Assert.Equal(40, windowWithArt.Suggestion1IllustrationControl.Size);
        Assert.True(windowWithArt.Suggestion2IllustrationControl.IsVisible);
        Assert.Equal(40, windowWithArt.Suggestion2IllustrationControl.Size);

        windowWithArt.AllowClose = true;
        windowWithArt.Close();

        // 2. Suggestions without art
        var infoNoArt = new BreakInfo(
            Kind: BreakKind.Long,
            ExerciseId: "test-no-art-1",
            TotalDuration: TimeSpan.FromMinutes(5),
            SuggestionExerciseIds: ["test-no-art-1", "test-no-art-2"]);

        var windowNoArt = new LongBreakOverlayWindow(
            isPrimary: true,
            info: infoNoArt,
            strings: _strings,
            onSkip: () => { });

        windowNoArt.Show();

        Assert.False(windowNoArt.Suggestion1IllustrationControl.IsVisible);
        Assert.False(windowNoArt.Suggestion2IllustrationControl.IsVisible);

        windowNoArt.AllowClose = true;
        windowNoArt.Close();
    }
}
