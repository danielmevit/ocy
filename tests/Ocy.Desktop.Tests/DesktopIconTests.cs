using System.Diagnostics;
using Avalonia.Headless.XUnit;
using Avalonia.Platform;
using Ocy.Desktop.Tray;
using Ocy.Desktop.Windows;
using Xunit;

namespace Ocy.Desktop.Tests;

public class DesktopIconTests
{
    [AvaloniaFact]
    public void AppIcon_ResourceLoads_AndReturnsValidWindowIcon()
    {
        // 1. Asset stream exists and can be opened
        using var stream = AssetLoader.Open(AppIcon.AssetUri);
        Assert.NotNull(stream);
        Assert.True(stream.Length > 0);

        // 2. Validate ICO header (reserved=0, type=1, count > 0)
        Span<byte> header = stackalloc byte[6];
        var bytesRead = stream.Read(header);
        Assert.Equal(6, bytesRead);
        var reserved = BitConverter.ToUInt16(header[..2]);
        var type = BitConverter.ToUInt16(header[2..4]);
        var count = BitConverter.ToUInt16(header[4..6]);
        Assert.Equal(0, reserved);
        Assert.Equal(1, type);
        Assert.True(count >= 1);

        // 3. AppIcon.GetIcon returns cached WindowIcon
        var icon = AppIcon.GetIcon();
        Assert.NotNull(icon);
        Assert.Same(icon, AppIcon.GetIcon());
    }

    [AvaloniaTheory]
    [InlineData(TrayIconState.Active, true, "light", "tray-active.ico")]
    [InlineData(TrayIconState.Paused, true, "light", "tray-paused.ico")]
    [InlineData(TrayIconState.OnBreak, true, "light", "tray-onbreak.ico")]
    [InlineData(TrayIconState.Attention, true, "light", "tray-attention.ico")]
    [InlineData(TrayIconState.Active, false, "dark", "tray-active.ico")]
    [InlineData(TrayIconState.Paused, false, "dark", "tray-paused.ico")]
    [InlineData(TrayIconState.OnBreak, false, "dark", "tray-onbreak.ico")]
    [InlineData(TrayIconState.Attention, false, "dark", "tray-attention.ico")]
    public void TrayIconRenderer_EveryStateAndSet_LoadsValidIconFromResource(
        TrayIconState state,
        bool lightTaskbar,
        string folder,
        string expectedFileName)
    {
        var resourceUri = new Uri($"avares://Ocy/Assets/Tray/{folder}/{expectedFileName}");
        using var stream = AssetLoader.Open(resourceUri);
        Assert.NotNull(stream);
        Assert.True(stream.Length > 0);

        var icon = TrayIconRenderer.GetIcon(state, lightTaskbar);
        Assert.NotNull(icon);

        // Cached
        var icon2 = TrayIconRenderer.GetIcon(state, lightTaskbar);
        Assert.Same(icon, icon2);

        // Verify that light and dark sets return distinct instances
        var oppositeIcon = TrayIconRenderer.GetIcon(state, !lightTaskbar);
        Assert.NotNull(oppositeIcon);
        Assert.NotSame(icon, oppositeIcon);
    }

    [AvaloniaFact]
    public void SettingsWindow_AndBreakWindows_HaveAppIcon()
    {
        var expected = AppIcon.GetIcon();
        Assert.NotNull(expected);

        var settings = new SettingsWindow();
        Assert.NotNull(settings.Icon);
        Assert.Same(expected, settings.Icon);

        var shortBreak = new ShortBreakWindow();
        Assert.NotNull(shortBreak.Icon);
        Assert.Same(expected, shortBreak.Icon);

        var warning = new WarningWindow();
        Assert.NotNull(warning.Icon);
        Assert.Same(expected, warning.Icon);

        var longBreak = new LongBreakOverlayWindow();
        Assert.NotNull(longBreak.Icon);
        Assert.Same(expected, longBreak.Icon);

        var dim = new DimOverlayWindow();
        Assert.NotNull(dim.Icon);
        Assert.Same(expected, dim.Icon);

        var edgeFrame = new EdgeFrameWindow();
        Assert.NotNull(edgeFrame.Icon);
        Assert.Same(expected, edgeFrame.Icon);
    }

    [Fact]
    public void PackTrayIco_CheckMode_Passes()
    {
        var scriptPath = FindPackScript();
        Assert.True(File.Exists(scriptPath), $"Script not found at {scriptPath}");

        var startInfo = new ProcessStartInfo
        {
            FileName = "python3",
            ArgumentList = { scriptPath, "--check" },
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };

        using var process = Process.Start(startInfo)!;
        var stdout = process.StandardOutput.ReadToEnd();
        var stderr = process.StandardError.ReadToEnd();
        process.WaitForExit();

        Assert.True(
            process.ExitCode == 0,
            $"pack_tray_ico.py --check failed with code {process.ExitCode}.\nStdout: {stdout}\nStderr: {stderr}");
    }

    private static string FindPackScript()
    {
        var dir = AppContext.BaseDirectory;
        while (!string.IsNullOrEmpty(dir))
        {
            var candidate = Path.Combine(dir, "tools", "icons", "pack_tray_ico.py");
            if (File.Exists(candidate))
            {
                return candidate;
            }
            var parent = Path.GetDirectoryName(dir);
            if (parent == dir) break;
            dir = parent;
        }

        // Fallback relative to repository root
        return Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "../../../../../tools/icons/pack_tray_ico.py"));
    }
}
