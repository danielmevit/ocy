using Avalonia;
using Avalonia.Controls;
using Ocy.Desktop.Host;
using Ocy.Desktop.Layout;
using Ocy.Desktop.Platform;
using Ocy.Desktop.ScreenRendering;
using Ocy.Desktop.Smoke;

namespace Ocy.Desktop;

public static class Program
{
    public static SingleInstance? CurrentSingleInstance { get; set; }

    [STAThread]
    public static int Main(string[] args)
    {
        if (IsSmokeRequested(args))
        {
            return SmokeRunner.Run(args);
        }

        if (RenderScreensArgs.IsRequested(args))
        {
            return BuildAvaloniaApp()
                .StartWithClassicDesktopLifetime(args, ShutdownMode.OnExplicitShutdown);
        }

        if (ScreensArgs.IsRequested(args))
        {
            return BuildAvaloniaApp()
                .StartWithClassicDesktopLifetime(args, ShutdownMode.OnExplicitShutdown);
        }

        var dataDir = AppDataPaths.GetDataDirectory();
        using var singleInstance = SingleInstance.TryAcquire(dataDir);
        if (singleInstance == null)
        {
            return 0;
        }

        CurrentSingleInstance = singleInstance;

        return BuildAvaloniaApp()
            .StartWithClassicDesktopLifetime(args, ShutdownMode.OnExplicitShutdown);
    }

    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .LogToTrace();

    private static bool IsSmokeRequested(string[] args)
    {
        foreach (var arg in args)
        {
            if (string.Equals(arg, "--smoke", StringComparison.OrdinalIgnoreCase) ||
                arg.StartsWith("--smoke=", StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
        }

        return false;
    }
}
