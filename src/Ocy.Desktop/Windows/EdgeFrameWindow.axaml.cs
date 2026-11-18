using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Avalonia.Media;

namespace Ocy.Desktop.Windows;

public enum EdgeFrameVariant
{
    ShortBreak,
    Warning,
}

public partial class EdgeFrameWindow : Window
{
    private bool _allowClose;

    public bool AllowClose
    {
        get => _allowClose;
        set => _allowClose = value;
    }

    public EdgeFrameVariant Variant { get; }

    public Border FrameBorderControl => this.FindControl<Border>("FrameBorder")!;

    public EdgeFrameWindow()
        : this(EdgeFrameVariant.ShortBreak)
    {
    }

    public EdgeFrameWindow(EdgeFrameVariant variant)
    {
        Variant = variant;
        InitializeComponent();
        Icon = AppIcon.GetIcon();

        if (variant == EdgeFrameVariant.Warning)
        {
            Classes.Add("warning");
        }

        ApplyVariantBrush();
        ActualThemeVariantChanged += (_, _) => ApplyVariantBrush();

        Closing += (_, e) =>
        {
            if (!_allowClose)
            {
                e.Cancel = true;
            }
        };
    }

    private void ApplyVariantBrush()
    {
        var brushKey = Variant == EdgeFrameVariant.Warning ? "AttentionFrameBrush" : "EdgeFrameBrush";
        if (this.TryFindResource(brushKey, ActualThemeVariant, out var res) && res is IBrush brush)
        {
            FrameBorderControl.BorderBrush = brush;
        }
        else if (Application.Current != null && Application.Current.TryGetResource(brushKey, ActualThemeVariant, out var appRes) && appRes is IBrush appBrush)
        {
            FrameBorderControl.BorderBrush = appBrush;
        }
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
