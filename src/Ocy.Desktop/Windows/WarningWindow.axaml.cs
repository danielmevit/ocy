using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Ocy.Desktop.Localization;

namespace Ocy.Desktop.Windows;

public partial class WarningWindow : Window
{
    private readonly Strings? _strings;
    private readonly Action? _onStartNow;
    private readonly Action? _onPostpone;
    private readonly Action? _onSkip;
    private int _postponeMinutes;
    private bool _allowClose;

    public bool AllowClose
    {
        get => _allowClose;
        set => _allowClose = value;
    }

    public TextBlock TitleTextBlock => this.FindControl<TextBlock>("TitleText")!;
    public TextBlock StrictMarkerTextBlock => this.FindControl<TextBlock>("StrictMarkerText")!;
    public Border CardBorderControl => this.FindControl<Border>("CardBorder")!;
    public Button StartNowButtonControl => this.FindControl<Button>("StartNowButton")!;
    public Button PostponeButtonControl => this.FindControl<Button>("PostponeButton")!;
    public Button SkipButtonControl => this.FindControl<Button>("SkipButton")!;

    public WarningWindow()
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

    public WarningWindow(
        Strings strings,
        Action onStartNow,
        Action onPostpone,
        Action onSkip,
        int postponeMinutes = 3)
        : this()
    {
        _strings = strings ?? throw new ArgumentNullException(nameof(strings));
        _onStartNow = onStartNow ?? throw new ArgumentNullException(nameof(onStartNow));
        _onPostpone = onPostpone ?? throw new ArgumentNullException(nameof(onPostpone));
        _onSkip = onSkip ?? throw new ArgumentNullException(nameof(onSkip));
        _postponeMinutes = postponeMinutes;

        StrictMarkerTextBlock.Text = _strings.Get("warning.strictOn");

        StartNowButtonControl.Content = _strings.Get("warning.startNow");
        StartNowButtonControl.Click += (_, _) => _onStartNow();

        PostponeButtonControl.Content = _strings.Format("warning.postpone", ("minutes", _postponeMinutes));
        PostponeButtonControl.Click += (_, _) => _onPostpone();

        SkipButtonControl.Content = _strings.Get("warning.skip");
        SkipButtonControl.Click += (_, _) => _onSkip();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    public void UpdateCountdown(TimeSpan countdown, bool strictMode, int postponesLeft, int postponeMinutes)
    {
        _postponeMinutes = postponeMinutes;

        if (_strings != null)
        {
            TitleTextBlock.Text = _strings.Format("warning.title", ("time", Strings.FormatCountdown(countdown)));
            PostponeButtonControl.Content = _strings.Format("warning.postpone", ("minutes", _postponeMinutes));
            StrictMarkerTextBlock.Text = _strings.Get("warning.strictOn");
        }

        StrictMarkerTextBlock.IsVisible = strictMode;
        PostponeButtonControl.IsVisible = !strictMode && postponesLeft > 0;
        SkipButtonControl.IsVisible = !strictMode;
    }
}
