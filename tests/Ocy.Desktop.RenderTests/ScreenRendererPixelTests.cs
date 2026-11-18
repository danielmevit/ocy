using Avalonia.Headless.XUnit;
using Ocy.Desktop.ScreenRendering;
using SkiaSharp;
using Xunit;

namespace Ocy.Desktop.RenderTests;

public sealed class ScreenRendererPixelTests
{
    [AvaloniaFact]
    public void RenderScreens_ProducesCompleteRealPixelsInBothThemes()
    {
        var directory = Path.Combine(Path.GetTempPath(), "ocy-render-pixels-" + Guid.NewGuid().ToString("N"));

        try
        {
            Assert.Equal(0, ScreenRenderer.Run(["--render-screens=" + directory]));

            var names = new[]
            {
                "short-break.png",
                "short-break-palming.png",
                "warning.png",
                "long-break.png",
                "settings.png",
            };

            foreach (var theme in new[] { "light", "dark" })
            {
                foreach (var name in names)
                {
                    var path = Path.Combine(directory, theme, name);
                    Assert.True(File.Exists(path), $"missing {theme}/{name}");

                    var pixels = ReadPixels(path);
                    AssertNotBlank(pixels, pixels.Width, pixels.Height, $"{theme}/{name}");

                    if (name == "settings.png")
                    {
                        Assert.True(pixels.OpaqueCount > (pixels.Width * pixels.Height * 0.20),
                            $"{theme}/{name} should fill more than 20% of the window.");
                        Assert.True(pixels.DistinctCount >= 3,
                            $"{theme}/{name} should contain distinct text and background colours.");
                    }
                    else
                    {
                        AssertTransparentOuterBand(pixels, 4, $"{theme}/{name}");
                    }

                    if (name == "warning.png")
                    {
                        var lowerThirdStart = pixels.Height * 2 / 3;
                        var lowerOpaque = 0;
                        for (var y = lowerThirdStart; y < pixels.Height; y++)
                        {
                            for (var x = 0; x < pixels.Width; x++)
                            {
                                if (pixels.Values[(y * pixels.Width + x) * 4 + 3] != 0)
                                {
                                    lowerOpaque++;
                                }
                            }
                        }

                        Assert.True(lowerOpaque > 0, $"{theme}/warning.png should show the buttons in the lower third.");
                    }
                }
            }

            foreach (var name in names)
            {
                var lightPixels = ReadPixels(Path.Combine(directory, "light", name));
                var darkPixels = ReadPixels(Path.Combine(directory, "dark", name));
                AssertDifferent(lightPixels, darkPixels, name);
            }

            var manifestPath = Path.Combine(directory, "manifest.json");
            Assert.True(File.Exists(manifestPath));
            var manifest = System.Text.Json.JsonSerializer.Deserialize<ScreenManifest>(
                File.ReadAllText(manifestPath));
            Assert.NotNull(manifest);
            Assert.Equal(10, manifest.Files.Count);
        }
        finally
        {
            if (Directory.Exists(directory))
            {
                Directory.Delete(directory, recursive: true);
            }
        }
    }

    private sealed record PixelData(int Width, int Height, byte[] Values, int OpaqueCount, int DistinctCount);

    private static PixelData ReadPixels(string path)
    {
        using var bitmap = SKBitmap.Decode(path);
        Assert.NotNull(bitmap);
        var width = bitmap.Width;
        var height = bitmap.Height;
        var stride = width * 4;
        var values = new byte[stride * height];
        var opaque = 0;
        var distinct = new HashSet<uint>();
        for (var y = 0; y < height; y++)
        {
            for (var x = 0; x < width; x++)
            {
                var color = bitmap.GetPixel(x, y);
                var i = (y * stride) + (x * 4);
                values[i] = color.Red;
                values[i + 1] = color.Green;
                values[i + 2] = color.Blue;
                values[i + 3] = color.Alpha;
                if (color.Alpha != 0)
                {
                    opaque++;
                    distinct.Add((uint)(color.Red | (color.Green << 8) | (color.Blue << 16) | (color.Alpha << 24)));
                }
            }
        }

        return new PixelData(width, height, values, opaque, distinct.Count);
    }

    private static void AssertNotBlank(PixelData pixels, int width, int height, string name)
    {
        Assert.True(pixels.OpaqueCount > (width * height * 0.02), $"{name} should have at least 2% opaque pixels.");
        Assert.True(pixels.DistinctCount > 1, $"{name} should contain more than one colour.");
    }

    private static void AssertTransparentOuterBand(PixelData pixels, int band, string name)
    {
        for (var y = 0; y < band; y++)
        {
            for (var x = 0; x < pixels.Width; x++)
            {
                Assert.Equal(0, pixels.Values[(y * pixels.Width + x) * 4 + 3]);
                Assert.Equal(0, pixels.Values[((pixels.Height - 1 - y) * pixels.Width + x) * 4 + 3]);
            }
        }

        for (var y = 0; y < pixels.Height; y++)
        {
            for (var x = 0; x < band; x++)
            {
                Assert.Equal(0, pixels.Values[(y * pixels.Width + x) * 4 + 3]);
                Assert.Equal(0, pixels.Values[(y * pixels.Width + pixels.Width - 1 - x) * 4 + 3]);
            }
        }
    }

    private static void AssertDifferent(PixelData light, PixelData dark, string name)
    {
        Assert.Equal(light.Width, dark.Width);
        Assert.Equal(light.Height, dark.Height);
        var differences = 0;
        for (var i = 0; i < light.Values.Length; i += 4)
        {
            if (light.Values[i] != dark.Values[i] ||
                light.Values[i + 1] != dark.Values[i + 1] ||
                light.Values[i + 2] != dark.Values[i + 2] ||
                light.Values[i + 3] != dark.Values[i + 3])
            {
                differences++;
            }
        }

        Assert.True(differences > 0, $"light and dark {name} renders should differ.");
    }
}
