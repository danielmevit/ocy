using Avalonia.Headless.XUnit;
using Ocy.Core.Settings;
using Ocy.Desktop.Host;
using Xunit;

namespace Ocy.Desktop.Tests;

public sealed class ErrorLoggerTests
{
    [Fact]
    public void ErrorLogger_WritesExceptionToLogFile()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-log-test-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            var ex = new InvalidOperationException("Test error message");
            ErrorLogger.Log(tempDir, "Test context", ex);

            var logFile = Path.Combine(tempDir, "ocy-errors.log");
            Assert.True(File.Exists(logFile));

            var text = File.ReadAllText(logFile);
            Assert.Contains("Test context", text);
            Assert.Contains("Test error message", text);
            Assert.Contains(nameof(InvalidOperationException), text);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [Fact]
    public void ErrorLogger_RotatesLogFile_WhenExceedingCap()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-log-cap-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            var logFile = Path.Combine(tempDir, "ocy-errors.log");
            var oldLogFile = Path.Combine(tempDir, "ocy-errors.old.log");

            // Create a file >= 256 KB
            var dummyData = new byte[256 * 1024];
            Array.Fill(dummyData, (byte)'A');
            File.WriteAllBytes(logFile, dummyData);

            // Log another error
            ErrorLogger.Log(tempDir, "After rotation", new InvalidOperationException("Rotated entry"));

            Assert.True(File.Exists(oldLogFile), "Old log file must exist after rotation");
            Assert.True(File.Exists(logFile), "New log file must be created");

            var oldText = File.ReadAllText(oldLogFile);
            Assert.StartsWith("AAAA", oldText);

            var newText = File.ReadAllText(logFile);
            Assert.Contains("After rotation", newText);
            Assert.Contains("Rotated entry", newText);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [Fact]
    public void ErrorLogger_NeverThrows_OnNullOrInvalidPaths()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-log-nulltest-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            ErrorLogger.Log(tempDir, null, null);
            ErrorLogger.Log("/invalid/path/that/cannot/exist/?!*:\0", "context", new InvalidOperationException("fail"));
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }

    [AvaloniaFact]
    public void AppHost_GuardedSaveFailure_LogsToDataDir()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "ocy-host-log-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempDir);
        try
        {
            using var host = new AppHost(dataDir: tempDir);

            // Force save settings to fail by making settings.json.tmp a directory or read-only if possible,
            // or pass settings with invalid state or test SaveSettings exception handling
            var settingsTmp = Path.Combine(tempDir, "settings.json.tmp");
            Directory.CreateDirectory(settingsTmp); // writing a file to a path that is a directory will throw IOException

            var result = host.SaveSettings(new OcySettings());
            Assert.False(result);

            var logFile = Path.Combine(tempDir, "ocy-errors.log");
            Assert.True(File.Exists(logFile));
            var logContent = File.ReadAllText(logFile);
            Assert.Contains("AppHost: saving settings failed", logContent);
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                try
                {
                    Directory.Delete(tempDir, recursive: true);
                }
                catch
                {
                    // Best effort cleanup
                }
            }
        }
    }
}
