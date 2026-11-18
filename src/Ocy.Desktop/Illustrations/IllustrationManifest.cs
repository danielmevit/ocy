using System.Collections.Frozen;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Reflection;
using System.Text.Json;

namespace Ocy.Desktop.Illustrations;

public sealed record IllustrationInfo(
    int Frames,
    IReadOnlyList<int> Sequence,
    int StepMs,
    int Frozen)
{
    public TimeSpan StepDuration => TimeSpan.FromMilliseconds(StepMs);
}

public sealed class IllustrationManifest
{
    private const string EmbeddedResourceName = "Ocy.Illustrations.manifest.json";
    private readonly FrozenDictionary<string, IllustrationInfo> _exercises;

    public string Version { get; }
    public string Palette { get; }
    public IReadOnlyList<int> ViewBox { get; }

    private static readonly Lazy<IllustrationManifest> LazyInstance = new(LoadEmbedded);
    public static IllustrationManifest Instance => LazyInstance.Value;

    public IllustrationManifest(
        string version,
        string palette,
        IReadOnlyList<int> viewBox,
        IReadOnlyDictionary<string, IllustrationInfo> exercises)
    {
        Version = version ?? string.Empty;
        Palette = palette ?? string.Empty;
        ViewBox = viewBox ?? [];
        _exercises = (exercises ?? new Dictionary<string, IllustrationInfo>()).ToFrozenDictionary(StringComparer.Ordinal);
    }

    public bool TryGet(string? exerciseId, [NotNullWhen(true)] out IllustrationInfo? info)
    {
        if (string.IsNullOrEmpty(exerciseId))
        {
            info = null;
            return false;
        }

        return _exercises.TryGetValue(exerciseId, out info);
    }

    public static IllustrationManifest Parse(string json)
    {
        ArgumentNullException.ThrowIfNull(json);
        using var doc = JsonDocument.Parse(json);
        return LoadFromDocument(doc);
    }

    public static IllustrationManifest LoadFromStream(Stream stream)
    {
        ArgumentNullException.ThrowIfNull(stream);
        using var doc = JsonDocument.Parse(stream);
        return LoadFromDocument(doc);
    }

    public static IllustrationManifest LoadEmbedded()
    {
        var assembly = typeof(IllustrationManifest).Assembly;
        using var stream = assembly.GetManifestResourceStream(EmbeddedResourceName);
        if (stream == null)
        {
            throw new InvalidOperationException(
                $"Embedded resource '{EmbeddedResourceName}' not found in assembly '{assembly.FullName}'. " +
                $"Available: {string.Join(", ", assembly.GetManifestResourceNames())}");
        }

        return LoadFromStream(stream);
    }

    private static IllustrationManifest LoadFromDocument(JsonDocument doc)
    {
        var root = doc.RootElement;
        var version = root.TryGetProperty("version", out var v) ? v.GetString() ?? string.Empty : string.Empty;
        var palette = root.TryGetProperty("palette", out var p) ? p.GetString() ?? string.Empty : string.Empty;

        var viewBoxList = new List<int>();
        if (root.TryGetProperty("viewBox", out var vbElem) && vbElem.ValueKind == JsonValueKind.Array)
        {
            foreach (var item in vbElem.EnumerateArray())
            {
                viewBoxList.Add(item.GetInt32());
            }
        }

        var exercises = new Dictionary<string, IllustrationInfo>(StringComparer.Ordinal);
        if (root.TryGetProperty("exercises", out var exElem) && exElem.ValueKind == JsonValueKind.Object)
        {
            foreach (var prop in exElem.EnumerateObject())
            {
                var frames = prop.Value.GetProperty("frames").GetInt32();
                var stepMs = prop.Value.GetProperty("stepMs").GetInt32();
                var frozen = prop.Value.GetProperty("frozen").GetInt32();

                var seqList = new List<int>();
                foreach (var item in prop.Value.GetProperty("sequence").EnumerateArray())
                {
                    seqList.Add(item.GetInt32());
                }

                exercises[prop.Name] = new IllustrationInfo(frames, seqList, stepMs, frozen);
            }
        }

        return new IllustrationManifest(version, palette, viewBoxList, exercises);
    }
}
