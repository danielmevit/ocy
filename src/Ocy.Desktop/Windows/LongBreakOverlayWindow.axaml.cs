using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Input;
using Ocy.Abstractions;
using Ocy.Core.Engine;
using Ocy.Desktop.Controls;
using Ocy.Desktop.Localization;

namespace Ocy.Desktop.Windows;

public partial class LongBreakOverlayWindow : Window
{
    /// <summary>The card's fixed width (DIP).</summary>
    public const double CardWidth = 480;

    /// <summary>Below this content width the two suggestions stack into one column (DIP).</summary>
    public const double StackSuggestionsBelow = 420;

    private const double ScreenGutter = 16;

    private readonly Strings? _strings;
    private readonly Action? _onSkip;
    private readonly bool _isPrimary;
    private readonly IPlatformServices? _platformServices;
    private bool _allowClose;
    private bool _strictMode;

    public bool AllowClose
    {
        get => _allowClose;
        set => _allowClose = value;
    }

    public Border CardBorderControl => CardBorder;
    public Panel RootPanelControl => RootPanel;
    public TextBlock TitleTextBlock => TitleText;
    public TextBlock BodyTextBlock => BodyText;
    public TextBlock CountdownLabelTextBlock => CountdownLabelText;
    public TextBlock CountdownTextBlock => CountdownText;
    public StackPanel SuggestionsPanelControl => SuggestionsPanel;
    public Grid SuggestionsGridControl => SuggestionsGrid;
    public TextBlock Suggestion1TitleBlock => Suggestion1Title;
    public TextBlock Suggestion1BodyBlock => Suggestion1Body;
    public TextBlock Suggestion2TitleBlock => Suggestion2Title;
    public TextBlock Suggestion2BodyBlock => Suggestion2Body;
    public ExerciseIllustration Suggestion1IllustrationControl => Suggestion1Illustration;
    public ExerciseIllustration Suggestion2IllustrationControl => Suggestion2Illustration;
    public ProgressBar ProgressBarControl => BreakProgressBar;
    public Button SkipButtonControl => SkipButton;

    /// <summary>True when the suggestions are stacked in one column (narrow screens).</summary>
    public bool SuggestionsStacked { get; private set; }

    public LongBreakOverlayWindow()
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
        KeyDown += OnKeyDown;
        Opened += OnOpened;
        RootPanel.SizeChanged += (_, e) => ApplyResponsiveLayout(e.NewSize.Width);
    }

    public LongBreakOverlayWindow(
        bool isPrimary,
        BreakInfo? info,
        Strings strings,
        Action onSkip,
        bool strictMode = false,
        int? titleVariant = null,
        IPlatformServices? platformServices = null)
        : this()
    {
        _isPrimary = isPrimary;
        _strings = strings ?? throw new ArgumentNullException(nameof(strings));
        _onSkip = onSkip ?? throw new ArgumentNullException(nameof(onSkip));
        _strictMode = strictMode;
        _platformServices = platformServices;

        ShowActivated = isPrimary;
        CardBorder.IsVisible = isPrimary;

        if (isPrimary)
        {
            var variant = titleVariant ?? Random.Shared.Next(1, 6);
            TitleText.Text = _strings.Get($"longbreak.title.{variant}");
            BodyText.Text = _strings.Get($"longbreak.body.{variant}");
            CountdownLabelText.Text = _strings.Get("longbreak.countdownLabel");

            SkipButton.Content = _strings.Get("longbreak.skip");
            SkipButton.Click += (_, _) => _onSkip();

            PopulateSuggestions(info);

            var total = info?.TotalDuration ?? TimeSpan.Zero;
            UpdateCountdown(total, strictMode);
            UpdateProgress(total, total);
        }
    }

    private void OnOpened(object? sender, EventArgs e)
    {
        if (_isPrimary && !_strictMode && SkipButton.IsVisible)
        {
            SkipButton.Focus();
        }
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape && _isPrimary && !_strictMode)
        {
            _onSkip?.Invoke();
            e.Handled = true;
        }
    }

    private void PopulateSuggestions(BreakInfo? info)
    {
        if (_strings == null || info?.SuggestionExerciseIds == null || info.SuggestionExerciseIds.Count == 0)
        {
            SuggestionsPanel.IsVisible = false;
            return;
        }

        var ids = info.SuggestionExerciseIds;
        var has1 = FillSuggestion(ids.Count > 0 ? ids[0] : null, Suggestion1Panel, Suggestion1Illustration, Suggestion1Title, Suggestion1Body);
        var has2 = FillSuggestion(ids.Count > 1 ? ids[1] : null, Suggestion2Panel, Suggestion2Illustration, Suggestion2Title, Suggestion2Body);
        SuggestionsPanel.IsVisible = has1 || has2;
    }

    private bool FillSuggestion(string? id, Control panel, ExerciseIllustration illustration, TextBlock title, TextBlock body)
    {
        var has = !string.IsNullOrEmpty(id) && _strings != null;
        if (has)
        {
            title.Text = _strings!.Get($"exercise.{id}.title");
            body.Text = _strings.Get($"exercise.{id}.body");
            illustration.PlatformServices = _platformServices;
            illustration.ExerciseId = id;
        }
        else
        {
            illustration.ExerciseId = null;
        }

        panel.IsVisible = has;
        title.IsVisible = has;
        body.IsVisible = has;
        return has;
    }

    /// <summary>
    /// Fits the card to the available width and stacks the suggestions when the content gets narrower than
    /// <see cref="StackSuggestionsBelow"/>.
    /// </summary>
    internal void ApplyResponsiveLayout(double availableWidth)
    {
        if (availableWidth <= 0 || double.IsNaN(availableWidth))
        {
            return;
        }

        var cardWidth = Math.Min(CardWidth, Math.Max(0, availableWidth - (2 * ScreenGutter)));
        CardBorder.Width = cardWidth;

        var contentWidth = cardWidth - CardBorder.Padding.Left - CardBorder.Padding.Right;
        SetSuggestionsStacked(contentWidth < StackSuggestionsBelow);
    }

    private void SetSuggestionsStacked(bool stacked)
    {
        if (stacked == SuggestionsStacked)
        {
            return;
        }

        SuggestionsStacked = stacked;
        SuggestionsGrid.ColumnDefinitions = stacked ? new ColumnDefinitions("*") : new ColumnDefinitions("*,*");
        SuggestionsGrid.RowSpacing = stacked ? 10 : 0;
        Grid.SetColumn(Suggestion2Panel, stacked ? 0 : 1);
        Grid.SetRow(Suggestion2Panel, stacked ? 1 : 0);
    }

    /// <summary>Shows the remaining time as numerals only (m:ss); the full sentence goes to assistive tech.</summary>
    public void UpdateCountdown(TimeSpan remaining, bool strictMode)
    {
        _strictMode = strictMode;

        if (!_isPrimary)
        {
            return;
        }

        var time = Strings.FormatCountdown(remaining);
        CountdownText.Text = time;
        if (_strings != null)
        {
            AutomationProperties.SetName(CountdownText, _strings.Format("longbreak.countdown", ("time", time)));
        }

        ApplyStrictMode(strictMode);
    }

    /// <summary>Shows the elapsed part of the break on the footer progress bar.</summary>
    public void UpdateProgress(TimeSpan remaining, TimeSpan total)
    {
        if (!_isPrimary)
        {
            return;
        }

        double percent = 0;
        if (total > TimeSpan.Zero)
        {
            var elapsed = total - remaining;
            percent = Math.Clamp(elapsed.TotalSeconds / total.TotalSeconds * 100.0, 0, 100);
        }

        BreakProgressBar.Value = percent;
    }

    private void ApplyStrictMode(bool strictMode)
    {
        SkipButton.IsVisible = !strictMode;
        Grid.SetColumnSpan(BreakProgressBar, strictMode ? 2 : 1);
    }
}
