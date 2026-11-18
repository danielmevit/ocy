using System.Text.Json;
using System.Text.RegularExpressions;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Avalonia.VisualTree;
using Ocy.Abstractions;
using Ocy.Core.Persistence;
using Ocy.Core.Platform;
using Ocy.Core.Settings;
using Ocy.Desktop.Controls;
using Ocy.Desktop.Coordinating;
using Ocy.Desktop.Host;
using Ocy.Desktop.Localization;
using Ocy.Desktop.Windows;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed partial class SettingsWindowTests : IDisposable
{
    private readonly Strings _strings = new();
    private readonly string _tempDir;

    public SettingsWindowTests()
    {
        _tempDir = Path.Combine(Path.GetTempPath(), "ocy-settings-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_tempDir);
    }

    public void Dispose()
    {
        try
        {
            if (File.Exists(_tempDir))
            {
                File.Delete(_tempDir);
            }
            else if (Directory.Exists(_tempDir))
            {
                Directory.Delete(_tempDir, recursive: true);
            }
        }
        catch
        {
            // Best effort cleanup
        }
    }

    private AppHost CreateHost(IPlatformServices? platform = null) =>
        new(breakWindows: NullBreakWindows.Instance, settingsWindow: NullSettingsWindow.Instance, dataDir: _tempDir, platformServices: platform);

    private static NumericUpDown[] Numerics(SettingsWindow w) =>
    [
        w.ShortIntervalInput, w.ShortDurationInput,
        w.LongIntervalInput, w.LongDurationInput, w.LongWarningInput, w.LongPostponeInput, w.LongMaxPostponesInput,
        w.AwayThresholdInput,
    ];

    [AvaloniaFact]
    public void LiveChange_IsDebounced_ThenUpdatesEngineAndWritesSettingsFile()
    {
        using var host = CreateHost();
        var window = new SettingsWindow(host);
        Assert.False(window.HasPendingChanges, "Loading the current values must not schedule a save");
        Assert.Equal(10, (int)(window.ShortIntervalInput.Value ?? 0));

        window.ShortIntervalInput.Value = 25;
        window.StrictModeToggle.IsChecked = true;

        // Not applied yet: waiting for the debounce
        Assert.True(window.HasPendingChanges);
        Assert.Equal(10, host.Engine.Settings.ShortBreaks.IntervalMinutes);
        Assert.False(File.Exists(Path.Combine(_tempDir, "settings.json")));

        window.FlushPendingChanges();

        Assert.False(window.HasPendingChanges);
        Assert.Equal(25, host.Engine.Settings.ShortBreaks.IntervalMinutes);
        Assert.True(host.Engine.Settings.StrictMode);

        var loaded = new AppDataStore(_tempDir).LoadSettings();
        Assert.Equal(25, loaded.ShortBreaks.IntervalMinutes);
        Assert.True(loaded.StrictMode);
        Assert.False(window.SaveErrorText.IsVisible);
        Assert.Equal(TimeSpan.FromMilliseconds(400), SettingsWindow.ApplyDelay);
    }

    [AvaloniaFact]
    public void Closing_AppliesPendingChangeImmediately()
    {
        using var host = CreateHost();
        var window = new SettingsWindow(host);
        window.Show();

        window.LongDurationInput.Value = 7;
        Assert.True(window.HasPendingChanges);

        window.Close();

        Assert.False(window.HasPendingChanges);
        Assert.Equal(7, host.Engine.Settings.LongBreaks.DurationMinutes);
        Assert.Equal(7, new AppDataStore(_tempDir).LoadSettings().LongBreaks.DurationMinutes);
    }

    [AvaloniaFact]
    public void TryButtons_ApplyPendingChangesFirst()
    {
        using var host = CreateHost();
        var window = new SettingsWindow(host);

        window.ShortDurationInput.Value = 30;
        Assert.True(window.HasPendingChanges);

        window.TryShortBreakButton.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.False(window.HasPendingChanges);
        Assert.Equal(30, host.Engine.Settings.ShortBreaks.DurationSeconds);
    }

    [AvaloniaFact]
    public void NoSaveButton_OnlyTheTwoTryLinks()
    {
        using var host = CreateHost();
        var window = new SettingsWindow(host);
        window.Show();

        var buttons = window.GetVisualDescendants()
            .OfType<Button>()
            .Where(b => b is not ToggleButton && b.TemplatedParent == null && b.IsEffectivelyVisible)
            .ToList();

        Assert.Equal(2, buttons.Count);
        Assert.Contains(window.TryShortBreakButton, buttons);
        Assert.Contains(window.TryLongBreakButton, buttons);
        Assert.All(buttons, b => Assert.Contains("text", b.Classes));
        Assert.Equal(_strings.Get("settings.tryShortBreak"), window.TryShortBreakButton.Content);
        Assert.Equal(_strings.Get("settings.tryLongBreak"), window.TryLongBreakButton.Content);

        // The try links live inside their own sections
        Assert.Contains(window.TryShortBreakButton, window.ShortBreaksSectionCard.GetLogicalDescendants());
        Assert.Contains(window.TryLongBreakButton, window.LongBreaksSectionCard.GetLogicalDescendants());

        window.Close();
    }

    [AvaloniaFact]
    public void SaveFailure_ShowsInlineErrorWithErrorToken()
    {
        using var host = CreateHost();
        var window = new SettingsWindow(host);

        // Make the data directory unwritable: replace it with a file
        Directory.Delete(_tempDir, recursive: true);
        File.WriteAllText(_tempDir, "not a directory");

        window.ShortIntervalInput.Value = 20;
        window.FlushPendingChanges();

        Assert.True(window.SaveErrorText.IsVisible);
        Assert.Equal(_strings.Get("settings.saveFailed"), window.SaveErrorText.Text);
        Assert.Equal("ErrorBrush", FindResourceKey(window.SaveErrorText));

        // Once saving works again the message goes away
        File.Delete(_tempDir);
        Directory.CreateDirectory(_tempDir);
        window.ShortIntervalInput.Value = 21;
        window.FlushPendingChanges();
        Assert.False(window.SaveErrorText.IsVisible);
    }

    private static string? FindResourceKey(TextBlock block)
    {
        foreach (var key in new[] { "ErrorBrush", "TextBrush", "TextMutedBrush" })
        {
            if (block.TryFindResource(key, block.ActualThemeVariant, out var res) && ReferenceEquals(res, block.Foreground))
            {
                return key;
            }
        }

        return null;
    }

    [AvaloniaFact]
    public void HeaderToggle_DisablesDependentRows_WithoutHidingThem()
    {
        using var host = CreateHost();
        var window = new SettingsWindow(host);
        window.Show();

        Assert.True(window.ShortIntervalRow.IsEffectivelyEnabled);
        Assert.True(window.LongIntervalRow.IsEffectivelyEnabled);

        window.ShortBreaksEnabledToggle.IsChecked = false;

        foreach (var row in new Control[] { window.ShortIntervalRow, window.ShortDurationRow, window.ShortFullscreenRow })
        {
            Assert.False(row.IsEffectivelyEnabled);
            Assert.True(row.IsVisible);
        }

        Assert.False(window.ShortIntervalInput.IsEffectivelyEnabled);
        Assert.False(window.ShortFullscreenToggle.IsEffectivelyEnabled);
        Assert.True(window.ShortBreaksEnabledToggle.IsEffectivelyEnabled);
        Assert.True(window.LongIntervalRow.IsEffectivelyEnabled);

        window.LongBreaksEnabledToggle.IsChecked = false;
        foreach (var row in new Control[] { window.LongIntervalRow, window.LongDurationRow, window.LongWarningRow, window.LongPostponeRow, window.LongMaxPostponesRow })
        {
            Assert.False(row.IsEffectivelyEnabled);
            Assert.True(row.IsVisible);
        }

        window.FlushPendingChanges();
        Assert.False(host.Engine.Settings.ShortBreaks.Enabled);
        Assert.False(host.Engine.Settings.LongBreaks.Enabled);

        window.ShortBreaksEnabledToggle.IsChecked = true;
        Assert.True(window.ShortIntervalRow.IsEffectivelyEnabled);

        // Header toggles carry an accessible name (their visible label is the section header)
        Assert.Equal(_strings.Get("settings.shortBreaks.enabled.label"), Avalonia.Automation.AutomationProperties.GetName(window.ShortBreaksEnabledToggle));
        Assert.Equal(_strings.Get("settings.longBreaks.enabled.label"), Avalonia.Automation.AutomationProperties.GetName(window.LongBreaksEnabledToggle));

        window.Close();
    }

    [AvaloniaFact]
    public void Layout_NumericSizes_ToggleEdges_AndRowDensity()
    {
        using var host = CreateHost(new FakeAutostartPlatformServices());
        var window = new SettingsWindow(host);
        Assert.Equal(560, window.Width);
        Assert.Equal(640, window.Height);
        Assert.Equal(480, window.MinWidth);
        window.Height = 1600;
        window.Show();

        var numerics = Numerics(window);
        foreach (var input in numerics)
        {
            Assert.Equal(92, input.Bounds.Width, 1);
            Assert.Equal(28, input.Bounds.Height, 1);
            Assert.Equal(Location.Right, input.ButtonSpinnerLocation);
            Assert.Equal(13, input.FontSize);
        }

        var lefts = numerics.Select(n => Math.Round(n.TranslatePoint(default, window)!.Value.X, 1)).Distinct().ToList();
        Assert.Single(lefts);

        var toggles = window.GetVisualDescendants().OfType<ToggleSwitch>().Where(t => t.IsEffectivelyVisible).ToList();
        Assert.True(toggles.Count >= 8);
        var rights = toggles.Select(t => Math.Round(t.TranslatePoint(new Point(t.Bounds.Width, 0), window)!.Value.X, 1)).Distinct().ToList();
        Assert.Single(rights);
        Assert.All(toggles, t => Assert.Equal(40, t.Bounds.Width, 1));

        // Rows are dense: label + 2 + help + 6/6 padding (MinHeight 44) + the 1 px divider, nothing more
        // (about 46 DIP per row with the Windows UI font; the headless test font has other metrics).
        foreach (var row in window.GetVisualDescendants().OfType<SettingRow>().Where(r => r.IsEffectivelyVisible))
        {
            var label = row.GetVisualDescendants().OfType<TextBlock>().Single(t => t.Name == "PART_Label");
            var help = row.GetVisualDescendants().OfType<TextBlock>().Single(t => t.Name == "PART_Help");
            var control = row.GetVisualDescendants().OfType<Panel>().First(p => p.Width == SettingRow.ControlColumnWidth);
            Assert.Equal(SettingRow.ControlColumnWidth, control.Bounds.Width, 1);
            Assert.Equal(13, label.FontSize);
            Assert.Equal(12, help.FontSize);
            var text = label.Bounds.Height + 2 + help.Bounds.Height;
            var expected = Math.Max(44, Math.Max(text, control.Bounds.Height) + 12) + (row.ShowDivider ? 1 : 0);
            Assert.Equal(expected, row.Bounds.Height, 1);
        }

        // ScrollViewer uses Padding (scrollbar at the window edge), not Margin
        Assert.Equal(new Thickness(16, 10), window.SettingsScroll.Padding);
        Assert.Equal(default, window.SettingsScroll.Margin);

        // No Fluent Separator anywhere
        Assert.Empty(window.GetVisualDescendants().OfType<Separator>());

        window.Close();
    }

    [AvaloniaFact]
    public void Dividers_SkipFirstVisibleRow_InEverySection()
    {
        using var host = CreateHost(new FakeAutostartPlatformServices());
        var window = new SettingsWindow(host);

        foreach (var rows in new Panel[] { window.GeneralRows, window.ShortBreakRows, window.LongBreakRows, window.BehaviourRows })
        {
            var visible = rows.Children.OfType<SettingRow>().Where(r => r.IsVisible).ToList();
            Assert.NotEmpty(visible);
            Assert.False(visible[0].ShowDivider);
            Assert.All(visible.Skip(1), r => Assert.True(r.ShowDivider));
        }
    }

    [AvaloniaFact]
    public void Units_ComeFromStringsFile()
    {
        using var host = CreateHost();
        var window = new SettingsWindow(host);

        Assert.Equal(_strings.Get("unit.minutes"), window.ShortIntervalRow.Unit);
        Assert.Equal(_strings.Get("unit.minutes"), window.LongIntervalRow.Unit);
        Assert.Equal(_strings.Get("unit.minutes"), window.LongDurationRow.Unit);
        Assert.Equal(_strings.Get("unit.minutes"), window.LongPostponeRow.Unit);
        Assert.Equal(_strings.Get("unit.seconds"), window.ShortDurationRow.Unit);
        Assert.Equal(_strings.Get("unit.seconds"), window.LongWarningRow.Unit);
        Assert.Equal(_strings.Get("unit.seconds"), window.AwayThresholdRow.Unit);
        Assert.True(string.IsNullOrEmpty(window.LongMaxPostponesRow.Unit));
    }

    [AvaloniaFact]
    public void NoLiterals_EveryVisibleTextComesFromStringsFile()
    {
        using var host = CreateHost(new FakeAutostartPlatformServices());
        var window = new SettingsWindow(host);
        window.Height = 1600;
        window.Show();

        var allowed = new HashSet<string>(LoadStringsJson().Values, StringComparer.Ordinal)
        {
            _strings.Format("about.version", ("name", _strings.Get("app.name")), ("version", SettingsWindow.GetAppVersion())),
        };

        var texts = window.GetVisualDescendants()
            .OfType<TextBlock>()
            .Where(t => t.IsEffectivelyVisible && !string.IsNullOrEmpty(t.Text))
            .Where(t => t.FindAncestorOfType<NumericUpDown>() == null)
            .Select(t => t.Text!)
            .ToList();

        Assert.NotEmpty(texts);
        Assert.All(texts, t => Assert.Contains(t, allowed));

        foreach (var row in window.GetVisualDescendants().OfType<SettingRow>())
        {
            Assert.Contains(row.Label!, allowed);
            Assert.Contains(row.Help!, allowed);
        }

        Assert.Equal(_strings.Format("settings.windowTitle", ("app", _strings.Get("app.name"))), window.Title);
        Assert.Equal($"{_strings.Get("app.name")} {SettingsWindow.GetAppVersion()}", window.AboutAppNameVersion.Text);
        Assert.Equal(_strings.Get("app.tagline"), window.AboutTagline.Text);
        Assert.NotNull(window.Icon);

        window.Close();
    }

    [Fact]
    public void StringKeys_UsedInXamlAndCode_ExistInStringsFile()
    {
        var json = LoadStringsJson();
        var root = FindRepoRoot();
        var xamlFiles = new[]
        {
            Path.Combine(root, "src", "Ocy.Desktop", "Windows", "SettingsWindow.axaml"),
            Path.Combine(root, "src", "Ocy.Desktop", "Windows", "LongBreakOverlayWindow.axaml"),
        };

        var keys = new List<string>();
        foreach (var file in xamlFiles)
        {
            keys.AddRange(StrMarkup().Matches(File.ReadAllText(file)).Select(m => m.Groups[1].Value));
        }

        Assert.True(keys.Count > 40, $"expected the settings XAML to use loc:Str for its text, found {keys.Count}");

        keys.AddRange([
            "about.version", "app.name", "app.tagline", "longbreak.countdown", "longbreak.countdownLabel", "longbreak.skip",
            "settings.windowTitle", "settings.saveFailed", "settings.autostart.failed",
        ]);

        Assert.All(keys, k => Assert.True(json.ContainsKey(k), $"missing strings key '{k}'"));

        // The removed Save model leaves no keys behind
        Assert.False(json.ContainsKey("settings.save"));
        Assert.False(json.ContainsKey("settings.saved"));
        Assert.False(json.ContainsKey("settings.section.try"));
        Assert.Equal("{name} {version}", json["about.version"]);
    }

    [GeneratedRegex(@"\{loc:Str ([A-Za-z0-9._-]+)\}")]
    private static partial Regex StrMarkup();

    [AvaloniaFact]
    public void UnbuiltFeatures_StayHidden()
    {
        using var host = CreateHost();
        var window = new SettingsWindow(host);

        Assert.False(window.LanguageRow.IsVisible);
        Assert.False(window.SoundsRow.IsVisible);
        Assert.False(window.StatusNotificationsRow.IsVisible);
    }

    [AvaloniaFact]
    public void AutostartUnsupported_RowHidden_GeneralCollapses_AndStoredValueKept()
    {
        new AppDataStore(_tempDir).SaveSettings(new OcySettings { StartWithSystem = true });

        using var host = CreateHost(UnsupportedPlatformServices.Instance);
        Assert.True(host.Engine.Settings.StartWithSystem);

        var window = new SettingsWindow(host);
        Assert.False(window.StartWithSystemRow.IsVisible);
        Assert.False(window.GeneralSectionCard.IsVisible, "General card must collapse when all rows are hidden");

        window.ShortIntervalInput.Value = 12;
        window.FlushPendingChanges();

        var loaded = new AppDataStore(_tempDir).LoadSettings();
        Assert.True(loaded.StartWithSystem, "StartWithSystem must stay true when autostart is unsupported");
        Assert.Equal(12, loaded.ShortBreaks.IntervalMinutes);
    }

    [AvaloniaFact]
    public void AutostartSupported_ChangeApplies_AndFailureShowsInlineMessageAndReverts()
    {
        var platform = new FakeAutostartPlatformServices();
        var autostart = (FakeAutostartService)platform.Autostart;
        autostart.Enabled = false;

        using var host = CreateHost(platform);
        var window = new SettingsWindow(host);
        Assert.True(window.StartWithSystemRow.IsVisible);
        Assert.True(window.GeneralSectionCard.IsVisible);
        Assert.False(window.StartWithSystemToggle.IsChecked);

        window.StartWithSystemToggle.IsChecked = true;
        Assert.True(autostart.Enabled);
        window.FlushPendingChanges();
        Assert.True(host.Engine.Settings.StartWithSystem);
        Assert.False(window.StartWithSystemError.IsVisible);

        autostart.ThrowOnSet = true;
        window.StartWithSystemToggle.IsChecked = false;
        Assert.True(window.StartWithSystemError.IsVisible);
        Assert.Equal(_strings.Get("settings.autostart.failed"), window.StartWithSystemError.Text);
        Assert.True(window.StartWithSystemToggle.IsChecked, "the toggle reverts after a failed change");
        Assert.False(window.HasPendingChanges);
    }

    private static Dictionary<string, string> LoadStringsJson()
    {
        var path = Path.Combine(FindRepoRoot(), "content", "strings", "en.json");
        return JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(path))!;
    }

    private static string FindRepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir != null && !File.Exists(Path.Combine(dir.FullName, "Ocy.slnx")))
        {
            dir = dir.Parent;
        }

        return dir?.FullName ?? throw new InvalidOperationException("Repository root (Ocy.slnx) not found.");
    }
}
