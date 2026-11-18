using System.Text.Json;
using System.Text.Json.Serialization;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media.Imaging;
using Avalonia.Platform;
using Avalonia.Styling;
using Avalonia.Threading;
using Avalonia.Media;
using SkiaSharp;
using Ocy.Core.Engine;
using Ocy.Core.Settings;
using Ocy.Desktop.Host;
using Ocy.Desktop.Illustrations;
using Ocy.Desktop.Layout;
using Ocy.Desktop.Localization;
using Ocy.Desktop.Windows;

namespace Ocy.Desktop.ScreenRendering;

public sealed record ScreenManifest(
    [property: JsonPropertyName("files")] IReadOnlyList<ScreenManifestFile> Files);

public sealed record ScreenManifestFile(
    [property: JsonPropertyName("path")] string Path,
    [property: JsonPropertyName("theme")] string Theme,
    [property: JsonPropertyName("width")] int Width,
    [property: JsonPropertyName("height")] int Height);

public static class ScreenRenderer
{
    private const double Scale = 2.0;
    private static readonly JsonSerializerOptions ManifestJsonOptions = new() { WriteIndented = true };
    private static readonly (string Name, ThemeVariant Variant)[] Themes =
    [
        ("light", ThemeVariant.Light),
        ("dark", ThemeVariant.Dark),
    ];

    public static int Run(string[] args)
    {
        var directory = RenderScreensArgs.ParseDirectory(args);
        directory = string.IsNullOrWhiteSpace(directory)
            ? Environment.CurrentDirectory
            : Path.GetFullPath(directory);

        try
        {
            Directory.CreateDirectory(directory);
            var files = new List<ScreenManifestFile>();

            foreach (var (themeName, themeVariant) in Themes)
            {
                if (Application.Current != null)
                {
                    Application.Current.RequestedThemeVariant = themeVariant;
                }

                var themeDirectory = Path.Combine(directory, themeName);
                Directory.CreateDirectory(themeDirectory);

                RenderShortBreak(themeDirectory, themeName, themeVariant, "look-left-right", 2, 12, "short-break.png", files);
                RenderShortBreak(themeDirectory, themeName, themeVariant, "palming", 0, 12, "short-break-palming.png", files);
                RenderWarning(themeDirectory, themeName, themeVariant, files);
                RenderLongBreak(themeDirectory, themeName, themeVariant, files);
                RenderSettings(themeDirectory, themeName, themeVariant, files);
            }

            var manifest = new ScreenManifest(files);
            var json = JsonSerializer.Serialize(manifest, ManifestJsonOptions);
            File.WriteAllText(Path.Combine(directory, "manifest.json"), json + Environment.NewLine);
            return 0;
        }
        catch (Exception ex)
        {
            ErrorLogger.Log(null, "Screen renderer failed", ex);
            Console.Error.WriteLine($"Screen render failed: {ex.Message}");
            Console.Error.WriteLine(ex);
            return 1;
        }
    }

    private static void RenderShortBreak(
        string themeDirectory,
        string themeName,
        ThemeVariant themeVariant,
        string exerciseId,
        int frame,
        int secondsLeft,
        string fileName,
        ICollection<ScreenManifestFile> files)
    {
        var strings = Strings.Shared;
        var window = new ShortBreakWindow(strings, () => { }, isFullscreen: false, isInteractive: false);
        window.RequestedThemeVariant = themeVariant;
        window.ShowActivated = false;
        window.UpdateExercise(exerciseId);
        var frozenFrame = frame == 0
            ? IllustrationManifestFrame(exerciseId)
            : frame;
        window.IllustrationControl.SetFrozenFrame(frozenFrame);
        window.UpdateProgress(TimeSpan.FromSeconds(secondsLeft), TimeSpan.FromSeconds(15));

        LayoutAndRender(window, Path.Combine(themeDirectory, fileName), themeName, files);
    }

    private static int IllustrationManifestFrame(string exerciseId)
    {
        return IllustrationManifest.Instance.TryGet(exerciseId, out var info)
            ? info.Frozen
            : 1;
    }

    private static void RenderWarning(
        string themeDirectory,
        string themeName,
        ThemeVariant themeVariant,
        ICollection<ScreenManifestFile> files)
    {
        var strings = Strings.Shared;
        var window = new WarningWindow(strings, () => { }, () => { }, () => { }, postponeMinutes: 3);
        window.RequestedThemeVariant = themeVariant;
        window.ShowActivated = false;
        window.UpdateCountdown(TimeSpan.FromSeconds(42), strictMode: false, postponesLeft: 1, postponeMinutes: 3);

        LayoutAndRender(window, Path.Combine(themeDirectory, "warning.png"), themeName, files);
    }

    private static void RenderLongBreak(
        string themeDirectory,
        string themeName,
        ThemeVariant themeVariant,
        ICollection<ScreenManifestFile> files)
    {
        var strings = Strings.Shared;
        var info = new BreakInfo(
            BreakKind.Long,
            "window-far",
            TimeSpan.FromMinutes(5),
            ["window-far", "palming"]);
        var window = new LongBreakOverlayWindow(
            isPrimary: true,
            info,
            strings,
            () => { },
            strictMode: false,
            titleVariant: 1);
        window.RequestedThemeVariant = themeVariant;
        window.ShowActivated = false;
        window.UpdateCountdown(TimeSpan.FromSeconds(252), strictMode: false);
        window.UpdateProgress(TimeSpan.FromSeconds(252), TimeSpan.FromMinutes(5));
        window.Suggestion1IllustrationControl.SetFrozenFrame(1);
        window.Suggestion2IllustrationControl.SetFrozenFrame(4);

        LayoutAndRender(
            window,
            Path.Combine(themeDirectory, "long-break.png"),
            themeName,
            files);
    }

    private static void RenderSettings(
        string themeDirectory,
        string themeName,
        ThemeVariant themeVariant,
        ICollection<ScreenManifestFile> files)
    {
        var window = new SettingsWindow();
        window.RequestedThemeVariant = themeVariant;
        window.LoadFrom(new OcySettings());
        window.Width = 560;
        window.Height = 640;

        LayoutAndRender(window, Path.Combine(themeDirectory, "settings.png"), themeName, files);
    }

    private static void LayoutAndRender(
        Window window,
        string filePath,
        string theme,
        ICollection<ScreenManifestFile> files)
    {
        try
        {
            window.Position = new PixelPoint(-30000, -30000);
            window.ShowActivated = false;
            window.ShowInTaskbar = false;
            window.WindowDecorations = WindowDecorations.None;
            window.WindowStartupLocation = WindowStartupLocation.Manual;
            window.ApplyTemplate();

            if (window is ShortBreakWindow shortWindow)
            {
                window.SizeToContent = SizeToContent.Manual;
                window.Width = 456;
                shortWindow.CardBorderControl.Margin = new Thickness(32);
                shortWindow.CardBorderControl.Width = 360;
                shortWindow.CardBorderControl.HorizontalAlignment = HorizontalAlignment.Center;
                shortWindow.CardBorderControl.BoxShadow = default;
                shortWindow.CardBorderControl.Measure(new Size(456 - 64, double.PositiveInfinity));
                window.Height = shortWindow.CardBorderControl.DesiredSize.Height;
                window.Measure(new Size(456, window.Height));
                window.Arrange(new Rect(0, 0, 456, window.Height));
            }
            else if (window is WarningWindow warningWindow)
            {
                window.SizeToContent = SizeToContent.Manual;
                window.Width = 456;
                warningWindow.CardBorderControl.Margin = new Thickness(32);
                warningWindow.CardBorderControl.Width = 360;
                warningWindow.CardBorderControl.HorizontalAlignment = HorizontalAlignment.Center;
                warningWindow.CardBorderControl.BoxShadow = default;
                warningWindow.CardBorderControl.Measure(new Size(456 - 64, double.PositiveInfinity));
                window.Height = warningWindow.CardBorderControl.DesiredSize.Height;
                window.Measure(new Size(456, window.Height));
                window.Arrange(new Rect(0, 0, 456, window.Height));
            }
            else if (window is LongBreakOverlayWindow longWindow)
            {
                window.SizeToContent = SizeToContent.Manual;
                window.Width = 576;
                longWindow.RootPanelControl.Background = Brushes.Transparent;
                longWindow.CardBorderControl.Margin = new Thickness(32);
                longWindow.CardBorderControl.BoxShadow = default;
                longWindow.CardBorderControl.Measure(new Size(480, double.PositiveInfinity));
                window.Height = longWindow.CardBorderControl.DesiredSize.Height;
                longWindow.ApplyResponsiveLayout(576);
                window.Measure(new Size(576, window.Height));
                window.Arrange(new Rect(0, 0, 576, window.Height));
            }

            window.Show();
            Dispatcher.UIThread.RunJobs();
            window.ApplyTemplate();
            window.Measure(new Size(window.Width, window.Height));
            window.Arrange(new Rect(0, 0, window.Width, window.Height));
            Dispatcher.UIThread.RunJobs();

            var width = Math.Max(1, (int)Math.Ceiling(window.Width * Scale));
            var height = Math.Max(1, (int)Math.Ceiling(window.Height * Scale));
            var pixelSize = new PixelSize(width, height);
            using var bitmap = new RenderTargetBitmap(pixelSize, new Vector(96 * Scale, 96 * Scale));
            bitmap.Render(window);
            bitmap.Save(filePath, PngBitmapEncoderOptions.Default);

            if (IsBlankOrSingleColour(filePath))
            {
                File.Delete(filePath);
                throw new InvalidOperationException(
                    $"Screen render '{theme}/{Path.GetFileName(filePath)}' was blank or a single colour.");
            }

            var relativePath = Path.GetRelativePath(
                Path.GetDirectoryName(filePath) is { } parent
                    ? Path.GetDirectoryName(parent) ?? parent
                    : Directory.GetCurrentDirectory(),
                filePath).Replace('\\', '/');

            files.Add(new ScreenManifestFile(relativePath, theme, width, height));
        }
        finally
        {
            switch (window)
            {
                case ShortBreakWindow shortWindow:
                    shortWindow.AllowClose = true;
                    break;
                case WarningWindow warningWindow:
                    warningWindow.AllowClose = true;
                    break;
                case LongBreakOverlayWindow longWindow:
                    longWindow.AllowClose = true;
                    break;
            }

            if (window.IsVisible)
            {
                window.Close();
            }
        }
    }

    private static bool IsBlankOrSingleColour(string filePath)
    {
        using var bitmap = SKBitmap.Decode(filePath);
        if (bitmap == null)
        {
            return true;
        }

        var distinct = new HashSet<uint>();
        var opaquePixels = 0;
        for (var y = 0; y < bitmap.Height; y++)
        {
            for (var x = 0; x < bitmap.Width; x++)
            {
                var color = bitmap.GetPixel(x, y);
                if (color.Alpha != 0)
                {
                    opaquePixels++;
                    distinct.Add((uint)((color.Red) | (color.Green << 8) | (color.Blue << 16) | (color.Alpha << 24)));
                }
            }
        }

        return opaquePixels == 0 || distinct.Count == 1;
    }
}
