using System.Text.Json;
using System.Text.Json.Serialization;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Threading;
using Ocy.Desktop.Host;
using Ocy.Desktop.Layout;
using Ocy.Desktop.ScreenRendering;

namespace Ocy.Desktop;

public sealed record ScreenJsonOutput(
    [property: JsonPropertyName("screens")] IReadOnlyList<ScreenJsonItem> Screens);

public sealed record ScreenJsonItem(
    [property: JsonPropertyName("x")] int X,
    [property: JsonPropertyName("y")] int Y,
    [property: JsonPropertyName("w")] int W,
    [property: JsonPropertyName("h")] int H,
    [property: JsonPropertyName("scaling")] double Scaling,
    [property: JsonPropertyName("primary")] bool Primary);

public partial class App : Application, IDisposable
{
    private AppHost? _host;

    public void Dispose()
    {
        _host?.Dispose();
        _host = null;
        GC.SuppressFinalize(this);
    }

    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);

        Dispatcher.UIThread.UnhandledException += (sender, e) =>
        {
            System.Diagnostics.Trace.TraceError($"App: unhandled UI thread exception: {e.Exception}");
            ErrorLogger.Log(_host?.DataDirectory, "Unhandled UI thread exception", e.Exception);
            e.Handled = true;
        };
    }

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            desktop.ShutdownMode = ShutdownMode.OnExplicitShutdown;

            var args = desktop.Args ?? Array.Empty<string>();
            if (RenderScreensArgs.IsRequested(args))
            {
                RunRenderScreens(desktop);
                base.OnFrameworkInitializationCompleted();
                return;
            }

            if (ScreensArgs.IsRequested(args))
            {
                RunScreens(desktop, args);
                base.OnFrameworkInitializationCompleted();
                return;
            }

            void QuitAction()
            {
                _host?.Dispose();
                Dispatcher.UIThread.Post(() => desktop.Shutdown(0));
            }

            _host = new AppHost(shutdownAction: QuitAction);
            Program.CurrentSingleInstance?.StartListening(() => _host.Coordinator.ShowSettings());
            _host.AttachTray(quitAction: QuitAction);

            if (_host.TrayController != null)
            {
                var icons = new TrayIcons { _host.TrayController.TrayIcon };
                TrayIcon.SetIcons(this, icons);
            }

            _host.Start();

            desktop.Exit += (_, _) =>
            {
                _host?.Dispose();
            };
        }

        base.OnFrameworkInitializationCompleted();
    }

    private static void RunRenderScreens(IClassicDesktopStyleApplicationLifetime desktop)
    {
        var args = desktop.Args ?? Array.Empty<string>();
        var exitCode = ScreenRenderer.Run(args);
        Dispatcher.UIThread.Post(() => desktop.Shutdown(exitCode));
    }

    private static void RunScreens(IClassicDesktopStyleApplicationLifetime desktop, string[] args)
    {
        var outFile = ScreensArgs.ParseOutFile(args);
        var screens = ScreenQuery.GetScreens();

        var items = screens.Select(s => new ScreenJsonItem(
            X: s.Bounds.X,
            Y: s.Bounds.Y,
            W: s.Bounds.Width,
            H: s.Bounds.Height,
            Scaling: s.Scaling,
            Primary: s.IsPrimary)).ToList();

        var result = new ScreenJsonOutput(items);
        var json = JsonSerializer.Serialize(result);

        Console.WriteLine(json);

        if (!string.IsNullOrWhiteSpace(outFile))
        {
            try
            {
                var dir = Path.GetDirectoryName(Path.GetFullPath(outFile));
                if (!string.IsNullOrEmpty(dir))
                {
                    Directory.CreateDirectory(dir);
                }

                File.WriteAllText(outFile, json + Environment.NewLine);
            }
            catch
            {
                // Best effort write
            }
        }

        Dispatcher.UIThread.Post(() => desktop.Shutdown(0));
    }
}
