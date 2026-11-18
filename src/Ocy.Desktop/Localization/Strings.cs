using System.Collections.Frozen;
using System.Reflection;
using System.Text.Json;

namespace Ocy.Desktop.Localization;

public sealed class Strings
{
    private const string ResourceName = "Ocy.Strings.en.json";
    private readonly FrozenDictionary<string, string> _strings;

    private static readonly Lazy<Strings> SharedInstance = new(() => new Strings());

    /// <summary>The app's string table, shared by XAML (<see cref="StrExtension"/>) and code.</summary>
    public static Strings Shared => SharedInstance.Value;

    public int KeyCount => _strings.Count;

    public Strings()
        : this(LoadEmbeddedStrings())
    {
    }

    public Strings(IReadOnlyDictionary<string, string> strings)
    {
        ArgumentNullException.ThrowIfNull(strings);
        _strings = strings.ToFrozenDictionary(StringComparer.Ordinal);
    }

    public string Get(string key)
    {
        ArgumentNullException.ThrowIfNull(key);
        return _strings.TryGetValue(key, out var value) ? value : key;
    }

    public string Format(string key, params (string Name, object? Value)[] args)
    {
        var template = Get(key);
        if (args.Length == 0)
        {
            return template;
        }

        var result = template;
        foreach (var (name, value) in args)
        {
            result = result.Replace("{" + name + "}", value?.ToString() ?? string.Empty, StringComparison.Ordinal);
        }

        return result;
    }

    public string Format(string key, (string Name, object? Value) arg)
    {
        var template = Get(key);
        return template.Replace("{" + arg.Name + "}", arg.Value?.ToString() ?? string.Empty, StringComparison.Ordinal);
    }

    public static string FormatCountdown(TimeSpan duration)
    {
        var totalSeconds = Math.Max(0, (int)Math.Floor(duration.TotalSeconds));
        var minutes = totalSeconds / 60;
        var seconds = totalSeconds % 60;
        return $"{minutes}:{seconds:D2}";
    }

    public string FormatTime(TimeSpan duration)
    {
        var h = Get("time.hours");
        var m = Get("time.minutes");
        return FormatTime(duration, h, m);
    }

    public static string TimeText(TimeSpan duration) => FormatTime(duration, "h", "min");

    public static string FormatTime(TimeSpan duration, string hoursUnit, string minutesUnit)
    {
        if (duration < TimeSpan.FromMinutes(1))
        {
            var seconds = Math.Max(0, (int)duration.TotalSeconds);
            return $"0:{seconds:D2}";
        }

        if (duration >= TimeSpan.FromHours(1))
        {
            var hours = (int)duration.TotalHours;
            var minutes = Math.Abs(duration.Minutes);
            return $"{hours} {hoursUnit} {minutes:D2} {minutesUnit}";
        }

        var totalMinutes = (int)duration.TotalMinutes;
        return $"{totalMinutes} {minutesUnit}";
    }

    private static Dictionary<string, string> LoadEmbeddedStrings()
    {
        var assembly = typeof(Strings).Assembly;
        using var stream = assembly.GetManifestResourceStream(ResourceName);
        if (stream == null)
        {
            throw new InvalidOperationException(
                $"Embedded resource '{ResourceName}' not found in assembly '{assembly.FullName}'. " +
                $"Available resources: {string.Join(", ", assembly.GetManifestResourceNames())}");
        }

        var dict = JsonSerializer.Deserialize<Dictionary<string, string>>(stream);
        return dict ?? throw new InvalidOperationException($"Failed to deserialize '{ResourceName}'.");
    }
}
