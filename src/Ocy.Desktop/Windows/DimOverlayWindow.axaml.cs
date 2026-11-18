using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Ocy.Desktop.Windows;

public partial class DimOverlayWindow : Window
{
    private bool _allowClose;

    public bool AllowClose
    {
        get => _allowClose;
        set => _allowClose = value;
    }

    public DimOverlayWindow()
    {
        InitializeComponent();
        Icon = AppIcon.GetIcon();
        Closing += (_, e) =>
        {
            if (!_allowClose)
            {
                e.Cancel = true;
            }
        };
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
