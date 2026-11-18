using Avalonia;
using Avalonia.Controls;

namespace Ocy.Desktop.Controls;

/// <summary>
/// One settings row: a label and an optional help line on the left, the editor (the row's content) plus an
/// optional unit in a fixed-width column on the right, and a 1 px divider above it. The look lives in the
/// <c>SettingRow</c> control theme in App.axaml.
/// </summary>
public class SettingRow : ContentControl
{
    /// <summary>Width of the right-hand control column (DIP).</summary>
    public const double ControlColumnWidth = 132;

    public static readonly StyledProperty<string?> LabelProperty =
        AvaloniaProperty.Register<SettingRow, string?>(nameof(Label));

    public static readonly StyledProperty<string?> HelpProperty =
        AvaloniaProperty.Register<SettingRow, string?>(nameof(Help));

    public static readonly StyledProperty<string?> UnitProperty =
        AvaloniaProperty.Register<SettingRow, string?>(nameof(Unit));

    public static readonly StyledProperty<bool> ShowDividerProperty =
        AvaloniaProperty.Register<SettingRow, bool>(nameof(ShowDivider), defaultValue: true);

    public string? Label
    {
        get => GetValue(LabelProperty);
        set => SetValue(LabelProperty, value);
    }

    public string? Help
    {
        get => GetValue(HelpProperty);
        set => SetValue(HelpProperty, value);
    }

    public string? Unit
    {
        get => GetValue(UnitProperty);
        set => SetValue(UnitProperty, value);
    }

    public bool ShowDivider
    {
        get => GetValue(ShowDividerProperty);
        set => SetValue(ShowDividerProperty, value);
    }

    /// <summary>
    /// Shows a divider above every visible row except the first one in <paramref name="rows"/>, so hidden rows
    /// never leave a stray or doubled line.
    /// </summary>
    public static void UpdateDividers(Panel rows)
    {
        ArgumentNullException.ThrowIfNull(rows);
        var first = true;
        foreach (var row in rows.Children.OfType<SettingRow>())
        {
            if (!row.IsVisible)
            {
                continue;
            }

            row.ShowDivider = !first;
            first = false;
        }
    }
}
