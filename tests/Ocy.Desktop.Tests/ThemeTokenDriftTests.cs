using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Media;
using Avalonia.Styling;
using Ocy.Desktop.Theme;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class ThemeTokenDriftTests
{
    private static Color GetColor(ThemeVariant theme, string key)
    {
        var app = Application.Current!;
        if (app.Resources.ThemeDictionaries.TryGetValue(theme, out var dict) && dict is IResourceDictionary rd)
        {
            if (rd.TryGetResource(key, theme, out var res) && res is Color c)
            {
                return c;
            }
        }

        throw new KeyNotFoundException($"Resource '{key}' not found in theme '{theme}'.");
    }

    private static SolidColorBrush GetBrush(ThemeVariant theme, string key)
    {
        var app = Application.Current!;
        if (app.Resources.ThemeDictionaries.TryGetValue(theme, out var dict) && dict is IResourceDictionary rd)
        {
            if (rd.TryGetResource(key, theme, out var res) && res is SolidColorBrush b)
            {
                return b;
            }
        }

        throw new KeyNotFoundException($"Brush '{key}' not found in theme '{theme}'.");
    }

    [AvaloniaFact]
    public void LightThemeResources_MatchDesignTokens()
    {
        // Core design tokens
        Assert.Equal(DesignTokens.LightBg, GetColor(ThemeVariant.Light, "BgColor"));
        Assert.Equal(DesignTokens.LightSurface, GetColor(ThemeVariant.Light, "SurfaceColor"));
        Assert.Equal(DesignTokens.LightText, GetColor(ThemeVariant.Light, "TextColor"));
        Assert.Equal(DesignTokens.LightTextMuted, GetColor(ThemeVariant.Light, "TextMutedColor"));
        Assert.Equal(DesignTokens.LightAccent, GetColor(ThemeVariant.Light, "AccentColor"));
        Assert.Equal(DesignTokens.LightOnAccent, GetColor(ThemeVariant.Light, "OnAccentColor"));
        Assert.Equal(DesignTokens.LightError, GetColor(ThemeVariant.Light, "ErrorColor"));
        Assert.Equal(DesignTokens.LightAttention, GetColor(ThemeVariant.Light, "AttentionColor"));
        Assert.Equal(DesignTokens.LightBorder, GetColor(ThemeVariant.Light, "BorderColor"));
        Assert.Equal(DesignTokens.LightOverlay, GetColor(ThemeVariant.Light, "OverlayColor"));

        // Fluent accent overrides
        Assert.Equal(DesignTokens.LightSystemAccent, GetColor(ThemeVariant.Light, "SystemAccentColor"));
        Assert.Equal(DesignTokens.LightSystemAccentLight1, GetColor(ThemeVariant.Light, "SystemAccentColorLight1"));
        Assert.Equal(DesignTokens.LightSystemAccentLight2, GetColor(ThemeVariant.Light, "SystemAccentColorLight2"));
        Assert.Equal(DesignTokens.LightSystemAccentLight3, GetColor(ThemeVariant.Light, "SystemAccentColorLight3"));
        Assert.Equal(DesignTokens.LightSystemAccentDark1, GetColor(ThemeVariant.Light, "SystemAccentColorDark1"));
        Assert.Equal(DesignTokens.LightSystemAccentDark2, GetColor(ThemeVariant.Light, "SystemAccentColorDark2"));
        Assert.Equal(DesignTokens.LightSystemAccentDark3, GetColor(ThemeVariant.Light, "SystemAccentColorDark3"));

        // Brushes
        Assert.Equal(DesignTokens.LightBg, GetBrush(ThemeVariant.Light, "BgBrush").Color);
        Assert.Equal(DesignTokens.LightSurface, GetBrush(ThemeVariant.Light, "SurfaceBrush").Color);
        Assert.Equal(DesignTokens.LightText, GetBrush(ThemeVariant.Light, "TextBrush").Color);
        Assert.Equal(DesignTokens.LightTextMuted, GetBrush(ThemeVariant.Light, "TextMutedBrush").Color);
        Assert.Equal(DesignTokens.LightAccent, GetBrush(ThemeVariant.Light, "AccentBrush").Color);
        Assert.Equal(DesignTokens.LightOnAccent, GetBrush(ThemeVariant.Light, "OnAccentBrush").Color);
        Assert.Equal(DesignTokens.LightError, GetBrush(ThemeVariant.Light, "ErrorBrush").Color);
        Assert.Equal(DesignTokens.LightAttention, GetBrush(ThemeVariant.Light, "AttentionBrush").Color);
        Assert.Equal(DesignTokens.LightAttention, GetBrush(ThemeVariant.Light, "AttentionFrameBrush").Color);
        Assert.Equal(0.55, GetBrush(ThemeVariant.Light, "AttentionFrameBrush").Opacity, 2);
        Assert.Equal(DesignTokens.LightBorder, GetBrush(ThemeVariant.Light, "BorderBrush").Color);
        Assert.Equal(DesignTokens.LightOverlay, GetBrush(ThemeVariant.Light, "OverlayBrush").Color);
        Assert.Equal(DesignTokens.LightDivider, GetColor(ThemeVariant.Light, "DividerColor"));
        Assert.Equal(DesignTokens.LightDivider, GetBrush(ThemeVariant.Light, "DividerBrush").Color);
    }

    [AvaloniaFact]
    public void DarkThemeResources_MatchDesignTokens()
    {
        // Core design tokens
        Assert.Equal(DesignTokens.DarkBg, GetColor(ThemeVariant.Dark, "BgColor"));
        Assert.Equal(DesignTokens.DarkSurface, GetColor(ThemeVariant.Dark, "SurfaceColor"));
        Assert.Equal(DesignTokens.DarkText, GetColor(ThemeVariant.Dark, "TextColor"));
        Assert.Equal(DesignTokens.DarkTextMuted, GetColor(ThemeVariant.Dark, "TextMutedColor"));
        Assert.Equal(DesignTokens.DarkAccent, GetColor(ThemeVariant.Dark, "AccentColor"));
        Assert.Equal(DesignTokens.DarkOnAccent, GetColor(ThemeVariant.Dark, "OnAccentColor"));
        Assert.Equal(DesignTokens.DarkError, GetColor(ThemeVariant.Dark, "ErrorColor"));
        Assert.Equal(DesignTokens.DarkAttention, GetColor(ThemeVariant.Dark, "AttentionColor"));
        Assert.Equal(DesignTokens.DarkBorder, GetColor(ThemeVariant.Dark, "BorderColor"));
        Assert.Equal(DesignTokens.DarkOverlay, GetColor(ThemeVariant.Dark, "OverlayColor"));

        // Fluent accent overrides
        Assert.Equal(DesignTokens.DarkSystemAccent, GetColor(ThemeVariant.Dark, "SystemAccentColor"));
        Assert.Equal(DesignTokens.DarkSystemAccentLight1, GetColor(ThemeVariant.Dark, "SystemAccentColorLight1"));
        Assert.Equal(DesignTokens.DarkSystemAccentLight2, GetColor(ThemeVariant.Dark, "SystemAccentColorLight2"));
        Assert.Equal(DesignTokens.DarkSystemAccentLight3, GetColor(ThemeVariant.Dark, "SystemAccentColorLight3"));
        Assert.Equal(DesignTokens.DarkSystemAccentDark1, GetColor(ThemeVariant.Dark, "SystemAccentColorDark1"));
        Assert.Equal(DesignTokens.DarkSystemAccentDark2, GetColor(ThemeVariant.Dark, "SystemAccentColorDark2"));
        Assert.Equal(DesignTokens.DarkSystemAccentDark3, GetColor(ThemeVariant.Dark, "SystemAccentColorDark3"));

        // Brushes
        Assert.Equal(DesignTokens.DarkBg, GetBrush(ThemeVariant.Dark, "BgBrush").Color);
        Assert.Equal(DesignTokens.DarkSurface, GetBrush(ThemeVariant.Dark, "SurfaceBrush").Color);
        Assert.Equal(DesignTokens.DarkText, GetBrush(ThemeVariant.Dark, "TextBrush").Color);
        Assert.Equal(DesignTokens.DarkTextMuted, GetBrush(ThemeVariant.Dark, "TextMutedBrush").Color);
        Assert.Equal(DesignTokens.DarkAccent, GetBrush(ThemeVariant.Dark, "AccentBrush").Color);
        Assert.Equal(DesignTokens.DarkOnAccent, GetBrush(ThemeVariant.Dark, "OnAccentBrush").Color);
        Assert.Equal(DesignTokens.DarkError, GetBrush(ThemeVariant.Dark, "ErrorBrush").Color);
        Assert.Equal(DesignTokens.DarkAttention, GetBrush(ThemeVariant.Dark, "AttentionBrush").Color);
        Assert.Equal(DesignTokens.DarkAttention, GetBrush(ThemeVariant.Dark, "AttentionFrameBrush").Color);
        Assert.Equal(0.55, GetBrush(ThemeVariant.Dark, "AttentionFrameBrush").Opacity, 2);
        Assert.Equal(DesignTokens.DarkBorder, GetBrush(ThemeVariant.Dark, "BorderBrush").Color);
        Assert.Equal(DesignTokens.DarkOverlay, GetBrush(ThemeVariant.Dark, "OverlayBrush").Color);
        Assert.Equal(DesignTokens.DarkDivider, GetColor(ThemeVariant.Dark, "DividerColor"));
        Assert.Equal(DesignTokens.DarkDivider, GetBrush(ThemeVariant.Dark, "DividerBrush").Color);
    }

    [Fact]
    public void DividerTokens_AreTextAtEightPercent()
    {
        Assert.Equal(20, DesignTokens.LightDivider.A);
        Assert.Equal(20, DesignTokens.DarkDivider.A);
        Assert.Equal(DesignTokens.LightText.R, DesignTokens.LightDivider.R);
        Assert.Equal(DesignTokens.LightText.G, DesignTokens.LightDivider.G);
        Assert.Equal(DesignTokens.LightText.B, DesignTokens.LightDivider.B);
        Assert.Equal(DesignTokens.DarkText.R, DesignTokens.DarkDivider.R);
        Assert.Equal(DesignTokens.DarkText.G, DesignTokens.DarkDivider.G);
        Assert.Equal(DesignTokens.DarkText.B, DesignTokens.DarkDivider.B);
    }

    [Fact]
    public void AttentionTokens_MatchSpecification_AndSatisfyTextContrast()
    {
        Assert.Equal(Color.Parse("#B04E2C"), DesignTokens.LightAttention);
        Assert.Equal(Color.Parse("#E08A64"), DesignTokens.DarkAttention);

        static double Luminance(Color c)
        {
            static double Srgb(double channel) => channel <= 0.03928 ? channel / 12.92 : Math.Pow((channel + 0.055) / 1.055, 2.4);
            return 0.2126 * Srgb(c.R / 255.0) + 0.7152 * Srgb(c.G / 255.0) + 0.0722 * Srgb(c.B / 255.0);
        }

        static double Contrast(Color c1, Color c2)
        {
            var l1 = Luminance(c1);
            var l2 = Luminance(c2);
            var brighter = Math.Max(l1, l2);
            var darker = Math.Min(l1, l2);
            return (brighter + 0.05) / (darker + 0.05);
        }

        var lightContrast = Contrast(DesignTokens.LightAttention, DesignTokens.LightSurface);
        Assert.True(lightContrast >= 4.5, $"Light attention contrast {lightContrast} must be >= 4.5:1 (AA)");

        var darkContrast = Contrast(DesignTokens.DarkAttention, DesignTokens.DarkSurface);
        Assert.True(darkContrast >= 4.5, $"Dark attention contrast {darkContrast} must be >= 4.5:1 (AA)");
    }

    [AvaloniaFact]
    public void CommonResources_MatchDesignTokens()
    {
        var app = Application.Current!;
        Assert.True(app.Resources.TryGetResource("CardCornerRadius", null, out var cardRadius));
        Assert.Equal(DesignTokens.CardRadius, (CornerRadius)cardRadius!);

        Assert.True(app.Resources.TryGetResource("ButtonCornerRadius", null, out var buttonRadius));
        Assert.Equal(DesignTokens.ButtonRadius, (CornerRadius)buttonRadius!);

        Assert.True(app.Resources.TryGetResource("AppFontFamily", null, out var fontFamily));
        Assert.Equal(DesignTokens.AppFontFamily.Name, ((FontFamily)fontFamily!).Name);
    }
}
