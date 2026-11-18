using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml;
using Avalonia.Media;
using Ocy.Abstractions;
using Ocy.Desktop.Controls;
using Ocy.Desktop.Localization;

namespace Ocy.Desktop.Windows;

public partial class ShortBreakWindow : Window
{
    private readonly Strings? _strings;
    private readonly Action? _onSkip;
    private readonly bool _isFullscreen;
    private readonly IPlatformServices? _platformServices;
    private bool _allowClose;

    public bool AllowClose
    {
        get => _allowClose;
        set => _allowClose = value;
    }

    public TextBlock TitleTextBlock => this.FindControl<TextBlock>("TitleText")!;
    public TextBlock BodyTextBlock => this.FindControl<TextBlock>("BodyText")!;
    public Button SkipButtonControl => this.FindControl<Button>("SkipButton")!;
    public ProgressBar ProgressBarControl => this.FindControl<ProgressBar>("BreakProgressBar")!;
    public Border CardBorderControl => this.FindControl<Border>("CardBorder")!;
    public Panel OverlayBackdropControl => this.FindControl<Panel>("OverlayBackdrop")!;
    public ExerciseIllustration IllustrationControl => this.FindControl<ExerciseIllustration>("Illustration")!;

    public ShortBreakWindow()
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

    public ShortBreakWindow(
        Strings strings,
        Action onSkip,
        bool isFullscreen = false,
        bool isInteractive = true,
        IPlatformServices? platformServices = null)
        : this()
    {
        _strings = strings ?? throw new ArgumentNullException(nameof(strings));
        _onSkip = onSkip ?? throw new ArgumentNullException(nameof(onSkip));
        _isFullscreen = isFullscreen;
        _platformServices = platformServices;

        IllustrationControl.PlatformServices = _platformServices;

        ToolTip.SetTip(SkipButtonControl, _strings.Get("shortbreak.skip"));
        AutomationProperties.SetName(SkipButtonControl, _strings.Get("shortbreak.skip"));
        if (isInteractive)
        {
            SkipButtonControl.Click += (_, _) => _onSkip();
        }
        else
        {
            ShowActivated = false;
            Focusable = false;
            IsHitTestVisible = false;
        }

        if (_isFullscreen)
        {
            ApplyFullscreenStyle();
        }
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    public void ApplyFullscreenStyle()
    {
        SizeToContent = SizeToContent.Manual;
        if (Application.Current?.TryGetResource("OverlayBrush", null, out var brush) == true && brush is IBrush overlay)
        {
            OverlayBackdropControl.Background = overlay;
        }

        OverlayBackdropControl.IsVisible = true;
        CardBorderControl.HorizontalAlignment = HorizontalAlignment.Center;
        CardBorderControl.VerticalAlignment = VerticalAlignment.Center;
        CardBorderControl.Width = 360;
    }

    public void UpdateExercise(string? exerciseId)
    {
        IllustrationControl.ExerciseId = exerciseId;

        if (_strings == null || string.IsNullOrWhiteSpace(exerciseId))
        {
            TitleTextBlock.Text = string.Empty;
            BodyTextBlock.Text = string.Empty;
            return;
        }

        TitleTextBlock.Text = _strings.Get($"exercise.{exerciseId}.title");
        BodyTextBlock.Text = _strings.Get($"exercise.{exerciseId}.body");
    }

    public void UpdateProgress(TimeSpan remaining, TimeSpan total)
    {
        if (total <= TimeSpan.Zero)
        {
            ProgressBarControl.Value = 0;
            return;
        }

        var ratio = remaining.TotalSeconds / total.TotalSeconds;
        var percentage = Math.Clamp(ratio * 100.0, 0.0, 100.0);
        ProgressBarControl.Value = percentage;
    }
}
