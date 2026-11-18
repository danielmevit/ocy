using Avalonia.Controls;
using Avalonia.Platform;

namespace Ocy.Desktop.Tray;

public static class TrayIconRenderer
{
    private static readonly Dictionary<(TrayIconState State, bool LightTaskbar), WindowIcon> _icons = new();
    private static readonly object _lock = new();

    public static WindowIcon GetIcon(TrayIconState state, bool lightTaskbar)
    {
        lock (_lock)
        {
            var key = (state, lightTaskbar);
            if (!_icons.TryGetValue(key, out var icon))
            {
                var folder = lightTaskbar ? "light" : "dark";
                var fileName = state switch
                {
                    TrayIconState.Active => "tray-active.ico",
                    TrayIconState.Paused => "tray-paused.ico",
                    TrayIconState.OnBreak => "tray-onbreak.ico",
                    TrayIconState.Attention => "tray-attention.ico",
                    _ => "tray-active.ico",
                };

                icon = LoadIcon(folder, fileName);
                _icons[key] = icon;
            }

            return icon;
        }
    }

    public static WindowIcon GetIcon(TrayIconState state) => GetIcon(state, lightTaskbar: true);

    private static WindowIcon LoadIcon(string folder, string fileName)
    {
        using var stream = AssetLoader.Open(new Uri($"avares://Ocy/Assets/Tray/{folder}/{fileName}"));
        return new WindowIcon(stream);
    }
}
