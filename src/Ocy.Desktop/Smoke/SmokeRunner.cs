using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization;
using Ocy.Abstractions;
using Ocy.Core.Persistence;
using Ocy.Core.Platform;
using Ocy.Desktop.Host;
using Ocy.Desktop.Localization;
using Ocy.Platform.Windows;

namespace Ocy.Desktop.Smoke;

public sealed record SmokeResult(
    [property: JsonPropertyName("app")] string App,
    [property: JsonPropertyName("version")] string Version,
    [property: JsonPropertyName("os")] string Os,
    [property: JsonPropertyName("platformSupported")] bool PlatformSupported,
    [property: JsonPropertyName("idleMs")] long IdleMs,
    [property: JsonPropertyName("focus")] string Focus,
    [property: JsonPropertyName("autostartEnabled")] bool AutostartEnabled,
    [property: JsonPropertyName("dataDir")] string DataDir,
    [property: JsonPropertyName("settingsLoaded")] bool SettingsLoaded,
    [property: JsonPropertyName("settingsState")] string SettingsState,
    [property: JsonPropertyName("stringsKeys")] int StringsKeys,
    [property: JsonPropertyName("error"), JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)] string? Error = null);

public static class SmokeRunner
{
    public static int Run(string[] args, string? overrideDataDir = null)
    {
        var outFile = ParseOutFile(args);
        var version = GetAppVersion();

        try
        {
            var dataDir = overrideDataDir ?? AppDataPaths.GetDataDirectory();
            var settingsPath = Path.Combine(dataDir, "settings.json");

            bool settingsLoaded = false;
            string settingsState = "missing";

            // Smoke uses its own read-only settings parsing rather than AppDataStore.LoadSettings
            // because AppDataStore.LoadSettings renames corrupt files to a backup on disk,
            // which smoke verification runs must never do.
            if (File.Exists(settingsPath))
            {
                try
                {
                    var json = File.ReadAllText(settingsPath);
                    var settings = JsonSerializer.Deserialize(json, OcyJsonContext.Default.OcySettings);
                    if (settings != null)
                    {
                        settingsLoaded = true;
                        settingsState = "ok";
                    }
                    else
                    {
                        settingsState = "corrupt";
                    }
                }
                catch
                {
                    settingsState = "corrupt";
                }
            }

            using IPlatformServices platform = OperatingSystem.IsWindows()
                ? new WindowsPlatformServices(Environment.ProcessPath ?? "Ocy.exe")
                : UnsupportedPlatformServices.Instance;

            var strings = new Strings();

            var idleMs = (long)platform.Idle.GetIdleTime().TotalMilliseconds;
            var focus = platform.Focus.GetState().ToString();
            var autostart = platform.Autostart.IsEnabled();
            var platformSupported = platform.Idle.IsSupported &&
                                    platform.Focus.IsSupported &&
                                    platform.Autostart.IsSupported &&
                                    platform.Session.IsSupported;

            var result = new SmokeResult(
                App: "Ocy",
                Version: version,
                Os: RuntimeInformation.OSDescription,
                PlatformSupported: platformSupported,
                IdleMs: idleMs,
                Focus: focus,
                AutostartEnabled: autostart,
                DataDir: dataDir,
                SettingsLoaded: settingsLoaded,
                SettingsState: settingsState,
                StringsKeys: strings.KeyCount);

            var jsonResult = JsonSerializer.Serialize(result);
            Console.WriteLine(jsonResult);

            WriteOutFile(outFile, jsonResult);

            return 0;
        }
        catch (Exception ex)
        {
            var errorResult = new SmokeResult(
                App: "Ocy",
                Version: version,
                Os: RuntimeInformation.OSDescription,
                PlatformSupported: false,
                IdleMs: 0,
                Focus: "Unknown",
                AutostartEnabled: false,
                DataDir: string.Empty,
                SettingsLoaded: false,
                SettingsState: "missing",
                StringsKeys: 0,
                Error: ex.ToString());

            var jsonResult = JsonSerializer.Serialize(errorResult);
            Console.WriteLine(jsonResult);

            WriteOutFile(outFile, jsonResult);

            return 1;
        }
    }

    private static string GetAppVersion()
    {
        var informational = typeof(SmokeRunner).Assembly
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?
            .InformationalVersion
            ?? typeof(SmokeRunner).Assembly.GetName().Version?.ToString(3)
            ?? "0.1.0";
        return informational.Split('+')[0];
    }

    private static string? ParseOutFile(string[] args)
    {
        for (var i = 0; i < args.Length; i++)
        {
            if (string.Equals(args[i], "--smoke", StringComparison.OrdinalIgnoreCase))
            {
                if (i + 1 < args.Length && !args[i + 1].StartsWith('-'))
                {
                    return args[i + 1];
                }

                return null;
            }

            if (args[i].StartsWith("--smoke=", StringComparison.OrdinalIgnoreCase))
            {
                return args[i]["--smoke=".Length..].Trim('"');
            }
        }

        return null;
    }

    private static void WriteOutFile(string? outFile, string json)
    {
        if (string.IsNullOrWhiteSpace(outFile))
        {
            return;
        }

        try
        {
            var dir = Path.GetDirectoryName(Path.GetFullPath(outFile));
            if (!string.IsNullOrEmpty(dir))
            {
                Directory.CreateDirectory(dir);
            }

            File.WriteAllText(outFile, json + Environment.NewLine);
        }
        catch
        {
            // Best effort write
        }
    }
}
