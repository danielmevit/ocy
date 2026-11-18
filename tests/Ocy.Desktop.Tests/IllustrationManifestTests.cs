using Ocy.Desktop.Illustrations;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class IllustrationManifestTests
{
    private static string FindRepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir != null && !File.Exists(Path.Combine(dir.FullName, "Ocy.slnx")))
        {
            dir = dir.Parent;
        }

        return dir?.FullName ?? throw new InvalidOperationException("Repo root (Ocy.slnx) not found.");
    }

    [Fact]
    public void EmbeddedManifest_LoadsExpectedExercises()
    {
        var manifest = IllustrationManifest.Instance;
        Assert.NotNull(manifest);
        Assert.Equal("v1f", manifest.Version);
        Assert.Equal("green", manifest.Palette);
        Assert.Equal([0, 0, 64, 64], manifest.ViewBox);

        // Every exercise in the source manifest (assets/illustrations/ocy-v1f/manifest.json) loads with the
        // same values; the art set changes over time, so compare against the source instead of literals.
        var root = FindRepoRoot();
        using var doc = System.Text.Json.JsonDocument.Parse(
            File.ReadAllText(Path.Combine(root, "assets", "illustrations", "ocy-v1f", "manifest.json")));
        var exercises = doc.RootElement.GetProperty("exercises");
        Assert.True(exercises.EnumerateObject().Any());
        foreach (var ex in exercises.EnumerateObject())
        {
            Assert.True(manifest.TryGet(ex.Name, out var info), ex.Name);
            Assert.NotNull(info);
            Assert.Equal(ex.Value.GetProperty("frames").GetInt32(), info.Frames);
            Assert.Equal(ex.Value.GetProperty("stepMs").GetInt32(), info.StepMs);
            Assert.Equal(TimeSpan.FromMilliseconds(info.StepMs), info.StepDuration);
            Assert.Equal(ex.Value.GetProperty("frozen").GetInt32(), info.Frozen);
            Assert.Equal(ex.Value.GetProperty("sequence").EnumerateArray().Select(e => e.GetInt32()).ToArray(), info.Sequence);
        }
    }

    [Fact]
    public void TryGet_MissingOrEmptyId_ReturnsFalse()
    {
        var manifest = IllustrationManifest.Instance;

        Assert.False(manifest.TryGet("nonexistent", out var info1));
        Assert.Null(info1);

        // An id without art (all 12 catalogue exercises have art since the full v1f set)
        Assert.False(manifest.TryGet("test-no-art", out var info2));
        Assert.Null(info2);

        Assert.False(manifest.TryGet(null, out var info3));
        Assert.Null(info3);

        Assert.False(manifest.TryGet(string.Empty, out var info4));
        Assert.Null(info4);
    }

    [Fact]
    public void Parse_CustomJson_ParsesCorrectly()
    {
        var json = """
        {
          "version": "test",
          "palette": "blue",
          "viewBox": [0, 0, 100, 100],
          "exercises": {
            "test-ex": {
              "frames": 2,
              "sequence": [1, 2],
              "stepMs": 150,
              "frozen": 2
            }
          }
        }
        """;

        var manifest = IllustrationManifest.Parse(json);
        Assert.Equal("test", manifest.Version);
        Assert.Equal("blue", manifest.Palette);
        Assert.Equal([0, 0, 100, 100], manifest.ViewBox);

        Assert.True(manifest.TryGet("test-ex", out var info));
        Assert.NotNull(info);
        Assert.Equal(2, info.Frames);
        Assert.Equal(150, info.StepMs);
        Assert.Equal(2, info.Frozen);
        Assert.Equal([1, 2], info.Sequence);
    }
}
