using Ocy.Desktop.Localization;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class StringsTests
{
    private readonly Strings _strings = new();

    [Fact]
    public void Get_ExistingKey_ReturnsValue()
    {
        Assert.Equal("Ocy", _strings.Get("app.name"));
        Assert.Equal("Settings", _strings.Get("tray.menu.settings"));
        Assert.Equal("Strict mode is on", _strings.Get("warning.strictOn"));
    }

    [Fact]
    public void Get_MissingKey_ReturnsKeyItself()
    {
        const string missingKey = "this.key.does.not.exist";
        Assert.Equal(missingKey, _strings.Get(missingKey));
    }

    [Fact]
    public void Format_ReplacesNamedPlaceholders()
    {
        var result = _strings.Format("longbreak.countdown", ("time", "23 min"));
        Assert.Equal("Back to your screen in 23 min", result);

        var postponeResult = _strings.Format("warning.postpone", ("minutes", 3));
        Assert.Equal("Postpone 3 min", postponeResult);
    }

    [Fact]
    public void Format_MissingKey_ReplacesNamedPlaceholdersInKey()
    {
        var result = _strings.Format("custom {name} reminder", ("name", "water"));
        Assert.Equal("custom water reminder", result);
    }

    [Theory]
    [InlineData(0, "0:00")]
    [InlineData(-10, "0:00")]
    [InlineData(5, "0:05")]
    [InlineData(42, "0:42")]
    [InlineData(59, "0:59")]
    public void TimeText_UnderOneMinute(int totalSeconds, string expected)
    {
        var duration = TimeSpan.FromSeconds(totalSeconds);
        Assert.Equal(expected, Strings.TimeText(duration));
    }

    [Theory]
    [InlineData(60, "1 min")]
    [InlineData(119, "1 min")]
    [InlineData(120, "2 min")]
    [InlineData(23 * 60, "23 min")]
    [InlineData(59 * 60 + 59, "59 min")]
    public void TimeText_OneMinuteOrMore_UnderOneHour(int totalSeconds, string expected)
    {
        var duration = TimeSpan.FromSeconds(totalSeconds);
        Assert.Equal(expected, Strings.TimeText(duration));
    }

    [Theory]
    [InlineData(60 * 60, "1 h 00 min")]
    [InlineData(65 * 60, "1 h 05 min")]
    [InlineData(123 * 60, "2 h 03 min")]
    [InlineData(10 * 3600 + 45 * 60, "10 h 45 min")]
    public void TimeText_OneHourOrMore(int totalSeconds, string expected)
    {
        var duration = TimeSpan.FromSeconds(totalSeconds);
        Assert.Equal(expected, Strings.TimeText(duration));
    }

    [Fact]
    public void FormatTime_UsesStringsUnits()
    {
        Assert.Equal("h", _strings.Get("time.hours"));
        Assert.Equal("min", _strings.Get("time.minutes"));
        Assert.Equal("1 h 05 min", _strings.FormatTime(TimeSpan.FromMinutes(65)));
        Assert.Equal("15 min", _strings.FormatTime(TimeSpan.FromMinutes(15)));
    }

    [Fact]
    public void SettingsWindowTitle_FormatsWithAppName()
    {
        var title = _strings.Format("settings.windowTitle", ("app", _strings.Get("app.name")));
        Assert.Equal("Ocy — Settings", title);
    }

    [Fact]
    public void KeyCount_MatchesExpected112()
    {
        Assert.Equal(112, _strings.KeyCount);
    }

    [Theory]
    [InlineData(5, "0:05")]
    [InlineData(263, "4:23")]
    [InlineData(1800, "30:00")]
    public void FormatCountdown_SeparateChecks(int totalSeconds, string expected)
    {
        var duration = TimeSpan.FromSeconds(totalSeconds);
        Assert.Equal(expected, Strings.FormatCountdown(duration));
    }
}
