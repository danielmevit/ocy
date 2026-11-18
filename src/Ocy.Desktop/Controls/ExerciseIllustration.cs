using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Threading;
using Ocy.Abstractions;
using Ocy.Desktop.Illustrations;

namespace Ocy.Desktop.Controls;

public class ExerciseIllustration : Control
{
    public static readonly StyledProperty<string?> ExerciseIdProperty =
        AvaloniaProperty.Register<ExerciseIllustration, string?>(nameof(ExerciseId));

    public static readonly StyledProperty<double> SizeProperty =
        AvaloniaProperty.Register<ExerciseIllustration, double>(nameof(Size), defaultValue: 56.0);

    public static readonly StyledProperty<IPlatformServices?> PlatformServicesProperty =
        AvaloniaProperty.Register<ExerciseIllustration, IPlatformServices?>(nameof(PlatformServices));

    public string? ExerciseId
    {
        get => GetValue(ExerciseIdProperty);
        set => SetValue(ExerciseIdProperty, value);
    }

    public double Size
    {
        get => GetValue(SizeProperty);
        set => SetValue(SizeProperty, value);
    }

    public IPlatformServices? PlatformServices
    {
        get => GetValue(PlatformServicesProperty);
        set => SetValue(PlatformServicesProperty, value);
    }

    public int CurrentFrame { get; private set; } = 1;

    public bool IsTimerRunning => _timer?.IsEnabled == true;

    public Drawing? CurrentDrawing => _currentDrawing;

    internal void SetFrozenFrame(int frame)
    {
        StopTimer();
        if (_info == null && !string.IsNullOrEmpty(ExerciseId) &&
            IllustrationManifest.Instance.TryGet(ExerciseId, out var info))
        {
            _info = info;
        }

        if (_info == null)
        {
            return;
        }

        CurrentFrame = Math.Clamp(frame, 1, _info.Frames);
        UpdateDrawing();
    }

    private DispatcherTimer? _timer;
    private IllustrationInfo? _info;
    private TimeSpan _elapsed = TimeSpan.Zero;
    private bool _isAttached;
    private Drawing? _currentDrawing;

    static ExerciseIllustration()
    {
        AffectsMeasure<ExerciseIllustration>(SizeProperty);
        AffectsRender<ExerciseIllustration>(SizeProperty);
    }

    public ExerciseIllustration()
    {
        ActualThemeVariantChanged += OnActualThemeVariantChanged;
        // Start collapsed until ExerciseId is set to an entry with art
        IsVisible = false;
    }

    protected override void OnAttachedToVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        _isAttached = true;
        UpdateTimerState();
        UpdateDrawing();
    }

    protected override void OnDetachedFromVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnDetachedFromVisualTree(e);
        _isAttached = false;
        StopTimer();
    }

    protected override void OnPropertyChanged(AvaloniaPropertyChangedEventArgs change)
    {
        base.OnPropertyChanged(change);

        if (change.Property == ExerciseIdProperty)
        {
            OnExerciseIdChanged();
        }
        else if (change.Property == PlatformServicesProperty)
        {
            OnPlatformServicesChanged();
        }
        else if (change.Property == IsVisibleProperty)
        {
            UpdateTimerState();
        }
    }

    private void OnActualThemeVariantChanged(object? sender, EventArgs e)
    {
        UpdateDrawing();
    }

    private void OnExerciseIdChanged()
    {
        _elapsed = TimeSpan.Zero;

        if (IllustrationManifest.Instance.TryGet(ExerciseId, out var info))
        {
            _info = info;
            IsVisible = true;

            if (PrefersReducedMotion())
            {
                CurrentFrame = info.Frozen;
                StopTimer();
            }
            else
            {
                CurrentFrame = FrameSequencer.GetFrame(info.Sequence, info.StepDuration, TimeSpan.Zero);
                UpdateTimerState();
            }
        }
        else
        {
            _info = null;
            IsVisible = false;
            StopTimer();
            _currentDrawing = null;
        }

        UpdateDrawing();
    }

    private void OnPlatformServicesChanged()
    {
        if (_info == null)
        {
            return;
        }

        if (PrefersReducedMotion())
        {
            CurrentFrame = _info.Frozen;
            StopTimer();
            UpdateDrawing();
        }
        else
        {
            UpdateTimerState();
        }
    }

    private bool PrefersReducedMotion()
    {
        return PlatformServices?.Motion.PrefersReducedMotion() == true;
    }

    private bool ShouldRunTimer()
    {
        if (!_isAttached || !IsVisible)
        {
            return false;
        }

        if (_info == null)
        {
            return false;
        }

        if (PrefersReducedMotion())
        {
            return false;
        }

        return true;
    }

    private void UpdateTimerState()
    {
        if (ShouldRunTimer())
        {
            if (_timer == null)
            {
                _timer = new DispatcherTimer();
                _timer.Tick += OnTimerTick;
            }

            _timer.Interval = _info!.StepDuration;
            if (!_timer.IsEnabled)
            {
                _timer.Start();
            }
        }
        else
        {
            StopTimer();
        }
    }

    private void StopTimer()
    {
        if (_timer?.IsEnabled == true)
        {
            _timer.Stop();
        }
    }

    private void OnTimerTick(object? sender, EventArgs e)
    {
        if (_info == null || !ShouldRunTimer())
        {
            StopTimer();
            return;
        }

        _elapsed += _info.StepDuration;
        CurrentFrame = FrameSequencer.GetFrame(_info.Sequence, _info.StepDuration, _elapsed);
        UpdateDrawing();
    }

    public void UpdateDrawing()
    {
        if (_info == null || string.IsNullOrEmpty(ExerciseId))
        {
            _currentDrawing = null;
            InvalidateVisual();
            return;
        }

        var isDark = ActualThemeVariant == ThemeVariant.Dark;
        var theme = isDark ? "dark" : "light";
        var key = $"Ill.{ExerciseId}.{theme}.f{CurrentFrame}";

        if (this.TryFindResource(key, ActualThemeVariant, out var res) && res is Drawing drawing)
        {
            _currentDrawing = drawing;
        }
        else if (Application.Current != null &&
                 Application.Current.TryFindResource(key, ActualThemeVariant, out var appRes) &&
                 appRes is Drawing appDrawing)
        {
            _currentDrawing = appDrawing;
        }
        else
        {
            _currentDrawing = null;
        }

        InvalidateVisual();
    }

    protected override Size MeasureOverride(Size availableSize)
    {
        if (!IsVisible)
        {
            return new Size(0, 0);
        }

        var s = Size;
        return new Size(s, s);
    }

    public override void Render(DrawingContext context)
    {
        base.Render(context);

        if (!IsVisible || _currentDrawing == null)
        {
            return;
        }

        var bounds = Bounds;
        if (bounds.Width <= 0 || bounds.Height <= 0)
        {
            return;
        }

        var scaleX = bounds.Width / 64.0;
        var scaleY = bounds.Height / 64.0;
        using (context.PushTransform(Matrix.CreateScale(scaleX, scaleY)))
        {
            _currentDrawing.Draw(context);
        }
    }
}
