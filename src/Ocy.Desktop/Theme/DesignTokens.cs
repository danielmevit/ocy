using Avalonia;
using Avalonia.Media;

namespace Ocy.Desktop.Theme;

public static class DesignTokens
{
    // Light
    public static readonly Color LightBg = Color.Parse("#F6F7F5");
    public static readonly Color LightSurface = Color.Parse("#FFFFFF");
    public static readonly Color LightText = Color.Parse("#1B1E1F");
    public static readonly Color LightTextMuted = Color.Parse("#5A6064");
    public static readonly Color LightAccent = Color.Parse("#2F7D62");
    public static readonly Color LightOnAccent = Color.Parse("#FFFFFF");
    public static readonly Color LightError = Color.Parse("#B3261E");
    public static readonly Color LightAttention = Color.Parse("#B04E2C");
    public static readonly Color LightBorder = Color.FromArgb(102, 90, 96, 100); // 40% of #5A6064
    public static readonly Color LightOverlay = Color.FromArgb(199, 0, 0, 0); // 78% of 255
    public static readonly Color LightDivider = Color.FromArgb(20, 27, 30, 31); // 8% of Text #1B1E1F

    // Fluent accent overrides (Light)
    public static readonly Color LightSystemAccent = Color.Parse("#2F7D62");
    public static readonly Color LightSystemAccentLight1 = Color.Parse("#469377");
    public static readonly Color LightSystemAccentLight2 = Color.Parse("#66AB91");
    public static readonly Color LightSystemAccentLight3 = Color.Parse("#8EC4AF");
    public static readonly Color LightSystemAccentDark1 = Color.Parse("#24654F");
    public static readonly Color LightSystemAccentDark2 = Color.Parse("#1A4D3C");
    public static readonly Color LightSystemAccentDark3 = Color.Parse("#11352A");

    // Dark
    public static readonly Color DarkBg = Color.Parse("#17191A");
    public static readonly Color DarkSurface = Color.Parse("#212426");
    public static readonly Color DarkText = Color.Parse("#ECEEEC");
    public static readonly Color DarkTextMuted = Color.Parse("#A4ABAF");
    public static readonly Color DarkAccent = Color.Parse("#5FBF97");
    public static readonly Color DarkOnAccent = Color.Parse("#0F1A15");
    public static readonly Color DarkError = Color.Parse("#FF8A80");
    public static readonly Color DarkAttention = Color.Parse("#E08A64");
    public static readonly Color DarkBorder = Color.FromArgb(77, 164, 171, 175); // 30% of #A4ABAF
    public static readonly Color DarkOverlay = Color.FromArgb(209, 0, 0, 0); // 82% of 255
    public static readonly Color DarkDivider = Color.FromArgb(20, 236, 238, 236); // 8% of Text #ECEEEC

    // Fluent accent overrides (Dark)
    public static readonly Color DarkSystemAccent = Color.Parse("#5FBF97");
    public static readonly Color DarkSystemAccentLight1 = Color.Parse("#78CBA9");
    public static readonly Color DarkSystemAccentLight2 = Color.Parse("#95D7BC");
    public static readonly Color DarkSystemAccentLight3 = Color.Parse("#B6E5D1");
    public static readonly Color DarkSystemAccentDark1 = Color.Parse("#4DA380");
    public static readonly Color DarkSystemAccentDark2 = Color.Parse("#3B8769");
    public static readonly Color DarkSystemAccentDark3 = Color.Parse("#2A6B53");

    // Common
    public static readonly FontFamily AppFontFamily = new("Segoe UI Variable, Segoe UI, -apple-system, BlinkMacSystemFont, Roboto, sans-serif");
    public static readonly CornerRadius CardRadius = new(8);
    public static readonly CornerRadius ButtonRadius = new(6);
}
