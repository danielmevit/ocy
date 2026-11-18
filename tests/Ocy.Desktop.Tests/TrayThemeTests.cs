using Avalonia.Headless.XUnit;
using Avalonia.Styling;
using Microsoft.Extensions.Time.Testing;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Core.Persistence;
using Ocy.Core.Platform;
using Ocy.Core.Settings;
using Ocy.Desktop.Coordinating;
using Ocy.Desktop.Host;
using Ocy.Desktop.Localization;
using Ocy.Desktop.Tray;
using Ocy.Desktop.Windows;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class FakeTaskbarThemeProvider : ITaskbarThemeProvider
{
    public bool IsSupported { get; set; } = true;

    public bool? LightTaskbarValue { get; set; }

    public int CallCount { get; private set; }

    public FakeTaskbarThemeProvider(bool? isLightTaskbar)
    {
        LightTaskbarValue = isLightTaskbar;
    }

    public bool? IsLightTaskbar()
    {
        CallCount++;
        return LightTaskbarValue;
    }
}

public sealed class FakeTaskbarPlatformServices : IPlatformServices
{
    public IUserIdleProvider Idle { get; } = UnsupportedPlatformServices.Instance.Idle;

    public IFocusStateProvider Focus { get; } = UnsupportedPlatformServices.Instance.Focus;

    public IAutostartService Autostart { get; } = UnsupportedPlatformServices.Instance.Autostart;

    public ISessionEventSource Session { get; } = UnsupportedPlatformServices.Instance.Session;

    public FakeTaskbarThemeProvider TaskbarThemeProvider { get; }

    public ITaskbarThemeProvider TaskbarTheme => TaskbarThemeProvider;

    public FakeTaskbarPlatformServices(bool? isLightTaskbar)
    {
        TaskbarThemeProvider = new FakeTaskbarThemeProvider(isLightTaskbar);
    }

    public void Dispose()
    {
    }
}

public sealed class TrayThemeTests
{
    [Theory]
    [InlineData(true, "Light", true)]
    [InlineData(true, "Dark", true)]
    [InlineData(false, "Light", false)]
    [InlineData(false, "Dark", false)]
    [InlineData(null, "Light", true)]
    [InlineData(null, "Dark", false)]
    public void TraySetSelector_ResolvesCorrectSet(bool? isLightTaskbar, string themeName, bool expectedUseLight)
    {
        var appTheme = themeName == "Light" ? ThemeVariant.Light : ThemeVariant.Dark;
        var result = TraySetSelector.UseLightSet(isLightTaskbar, appTheme);
        Assert.Equal(expectedUseLight, result);
    }

    [Fact]
    public void TraySetSelector_NullAppTheme_FallsBackToDark()
    {
        var result = TraySetSelector.UseLightSet(null, null);
        Assert.False(result);
    }

    [AvaloniaFact]
    public void FakePlatformServices_WithDarkTaskbarTheme_TrayUsesDarkSet()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-tray-" + Guid.NewGuid().ToString("N"));
        try
        {
            var fakePlatform = new FakeTaskbarPlatformServices(isLightTaskbar: false);
            var timeProvider = new FakeTimeProvider(new DateTimeOffset(2026, 11, 18, 10, 0, 0, TimeSpan.Zero));

            using var host = new AppHost(
                timeProvider: timeProvider,
                dataDir: tempDir,
                platformServices: fakePlatform);

            host.AttachTray(quitAction: () => { });

            Assert.NotNull(host.TrayController);
            Assert.False(host.TrayController.CurrentUseLightSet);

            var expectedDarkIcon = TrayIconRenderer.GetIcon(TrayIconState.Active, lightTaskbar: false);
            Assert.Same(expectedDarkIcon, host.TrayController.TrayIcon.Icon);
            Assert.Equal(1, fakePlatform.TaskbarThemeProvider.CallCount);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                Directory.Delete(tempDir, recursive: true);
            }
        }
    }

    [AvaloniaFact]
    public void FakePlatformServices_WithLightTaskbarTheme_TrayUsesLightSet()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-tray-" + Guid.NewGuid().ToString("N"));
        try
        {
            var fakePlatform = new FakeTaskbarPlatformServices(isLightTaskbar: true);
            var timeProvider = new FakeTimeProvider(new DateTimeOffset(2026, 11, 18, 10, 0, 0, TimeSpan.Zero));

            using var host = new AppHost(
                timeProvider: timeProvider,
                dataDir: tempDir,
                platformServices: fakePlatform);

            host.AttachTray(quitAction: () => { });

            Assert.NotNull(host.TrayController);
            Assert.True(host.TrayController.CurrentUseLightSet);

            var expectedLightIcon = TrayIconRenderer.GetIcon(TrayIconState.Active, lightTaskbar: true);
            Assert.Same(expectedLightIcon, host.TrayController.TrayIcon.Icon);
            Assert.Equal(1, fakePlatform.TaskbarThemeProvider.CallCount);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                Directory.Delete(tempDir, recursive: true);
            }
        }
    }

    [AvaloniaFact]
    public void TrayController_ReadsTaskbarThemeAtMostOncePerMinuteOnTick()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-tray-rate-" + Guid.NewGuid().ToString("N"));
        try
        {
            var fakeTheme = new FakeTaskbarThemeProvider(isLightTaskbar: false);
            var fakeTime = new FakeTimeProvider(new DateTimeOffset(2026, 11, 18, 10, 0, 0, TimeSpan.Zero));

            var store = new AppDataStore(tempDir, fakeTime);
            using var engine = new BreakEngine(
                settings: store.LoadSettings(),
                platformServices: UnsupportedPlatformServices.Instance,
                timeProvider: fakeTime,
                dataStore: store);

            var strings = new Strings();
            var presenter = new TrayPresenter(strings, fakeTime);
            var windows = new FakeBreakWindows();
            var settingsWindow = new FakeSettingsWindow();
            using var coordinator = new BreakUiCoordinator(engine, windows, settingsWindow);

            using var controller = new TrayController(
                engine: engine,
                coordinator: coordinator,
                presenter: presenter,
                quitAction: () => { },
                taskbarTheme: fakeTheme,
                timeProvider: fakeTime);

            // 1. Initially read at start
            Assert.Equal(1, fakeTheme.CallCount);
            Assert.False(controller.CurrentUseLightSet);

            var initialIcon = controller.TrayIcon.Icon;
            Assert.Same(TrayIconRenderer.GetIcon(TrayIconState.Active, lightTaskbar: false), initialIcon);

            // 2. Tick after 30 seconds -> within 60s, should NOT query provider again
            fakeTime.Advance(TimeSpan.FromSeconds(30));
            controller.Update(engine.Snapshot);
            Assert.Equal(1, fakeTheme.CallCount);

            // Icon must be the exact same reference (no swap/flicker)
            Assert.Same(initialIcon, controller.TrayIcon.Icon);

            // 3. User changed taskbar theme in OS to light
            fakeTheme.LightTaskbarValue = true;

            // Tick at 59 seconds -> still under 60s from start
            fakeTime.Advance(TimeSpan.FromSeconds(29));
            controller.Update(engine.Snapshot);
            Assert.Equal(1, fakeTheme.CallCount);
            Assert.False(controller.CurrentUseLightSet);

            // 4. Tick at 60 seconds -> exactly 60s elapsed, provider is queried
            fakeTime.Advance(TimeSpan.FromSeconds(1));
            controller.Update(engine.Snapshot);
            Assert.Equal(2, fakeTheme.CallCount);
            Assert.True(controller.CurrentUseLightSet);

            var newIcon = controller.TrayIcon.Icon;
            Assert.Same(TrayIconRenderer.GetIcon(TrayIconState.Active, lightTaskbar: true), newIcon);
            Assert.NotSame(initialIcon, newIcon);

            // 5. Subsequent update within the next minute does not query or swap
            controller.Update(engine.Snapshot);
            Assert.Equal(2, fakeTheme.CallCount);
            Assert.Same(newIcon, controller.TrayIcon.Icon);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                Directory.Delete(tempDir, recursive: true);
            }
        }
    }

    private sealed class FakeBreakWindows : IBreakWindows
    {
        public int CloseCount { get; private set; }

        public void ShowShortBreak(BreakInfo info) { }

        public void ShowWarning(TimeSpan countdown) { }

        public void ShowLongBreak(BreakInfo info) { }

        public void UpdateCountdown(EngineSnapshot snapshot) { }

        public void CloseAll() => CloseCount++;
    }

    private sealed class FakeSettingsWindow : ISettingsWindow
    {
        public int ShowCount { get; private set; }

        public void Show() => ShowCount++;
    }
}
