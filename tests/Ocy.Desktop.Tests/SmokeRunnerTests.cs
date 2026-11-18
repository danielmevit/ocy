using System.Text.Json;
using Ocy.Desktop.Smoke;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class SmokeRunnerTests
{
    [Fact]
    public void Smoke_WhenSettingsMissing_ReportsMissingAndDoesNotCreateFile()
    {
        var tempOutFile = Path.Combine(Path.GetTempPath(), "ocy-smoke-" + Guid.NewGuid().ToString("N") + ".json");
        var tempDataDir = Path.Combine(Path.GetTempPath(), "ocy-smoke-data-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDataDir);

        try
        {
            var exitCode = SmokeRunner.Run(["--smoke=" + tempOutFile], overrideDataDir: tempDataDir);
            Assert.Equal(0, exitCode);
            Assert.True(File.Exists(tempOutFile));

            var json = File.ReadAllText(tempOutFile);
            var result = JsonSerializer.Deserialize<SmokeResult>(json);
            Assert.NotNull(result);
            Assert.Equal("Ocy", result.App);
            Assert.False(string.IsNullOrWhiteSpace(result.Version));
            Assert.True(result.StringsKeys > 0);
            Assert.Equal("missing", result.SettingsState);
            Assert.False(result.SettingsLoaded);
        }
        finally
        {
            try
            {
                if (File.Exists(tempOutFile))
                {
                    File.Delete(tempOutFile);
                }
            }
            catch
            {
                // Best effort cleanup
            }

            try
            {
                if (Directory.Exists(tempDataDir))
                {
                    Directory.Delete(tempDataDir, recursive: true);
                }
            }
            catch
            {
                // Best effort cleanup
            }
        }
    }

    [Fact]
    public void Smoke_WhenSettingsPresent_ReportsOkAndLoaded()
    {
        var tempOutFile = Path.Combine(Path.GetTempPath(), "ocy-smoke-" + Guid.NewGuid().ToString("N") + ".json");
        var tempDataDir = Path.Combine(Path.GetTempPath(), "ocy-smoke-data-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDataDir);

        try
        {
            File.WriteAllText(Path.Combine(tempDataDir, "settings.json"), "{}");

            var exitCode = SmokeRunner.Run(["--smoke=" + tempOutFile], overrideDataDir: tempDataDir);
            Assert.Equal(0, exitCode);
            Assert.True(File.Exists(tempOutFile));

            var json = File.ReadAllText(tempOutFile);
            var result = JsonSerializer.Deserialize<SmokeResult>(json);
            Assert.NotNull(result);
            Assert.Equal("ok", result.SettingsState);
            Assert.True(result.SettingsLoaded);
        }
        finally
        {
            try
            {
                if (File.Exists(tempOutFile))
                {
                    File.Delete(tempOutFile);
                }
            }
            catch
            {
                // Best effort cleanup
            }

            try
            {
                if (Directory.Exists(tempDataDir))
                {
                    Directory.Delete(tempDataDir, recursive: true);
                }
            }
            catch
            {
                // Best effort cleanup
            }
        }
    }
}
