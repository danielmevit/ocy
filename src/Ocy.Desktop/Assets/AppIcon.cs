using Avalonia.Controls;
using Avalonia.Platform;

namespace Ocy.Desktop;

public static class AppIcon
{
    public static readonly Uri AssetUri = new("avares://Ocy/Assets/ocy.ico");
    private static WindowIcon? _icon;
    private static readonly object _lock = new();

    public static WindowIcon GetIcon()
    {
        if (_icon != null)
        {
            return _icon;
        }

        lock (_lock)
        {
            if (_icon != null)
            {
                return _icon;
            }

            using var stream = AssetLoader.Open(AssetUri);
            _icon = new WindowIcon(stream);
            return _icon;
        }
    }
}
