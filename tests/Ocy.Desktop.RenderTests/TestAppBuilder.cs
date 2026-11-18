using Avalonia;
using Avalonia.Headless;
using Avalonia.Skia;
using Ocy.Desktop;
using Xunit;

[assembly: AvaloniaTestApplication(typeof(Ocy.Desktop.RenderTests.TestAppBuilder))]
[assembly: CollectionBehavior(DisableTestParallelization = true)]

namespace Ocy.Desktop.RenderTests;

public static class TestAppBuilder
{
    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<App>()
            .UseSkia()
            .UseHeadless(new AvaloniaHeadlessPlatformOptions
            {
                UseHeadlessDrawing = false
            });
}
