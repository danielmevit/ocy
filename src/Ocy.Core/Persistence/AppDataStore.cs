using System.Text.Json;
using Ocy.Core.Engine;
using Ocy.Core.Settings;
using Ocy.Core.Statistics;

namespace Ocy.Core.Persistence;

public sealed class AppDataStore
{
    private readonly TimeProvider _timeProvider;

    public string DirectoryPath { get; }

    public string SettingsPath => Path.Combine(DirectoryPath, "settings.json");

    public string StatePath => Path.Combine(DirectoryPath, "state.json");

    public string StatsPath => Path.Combine(DirectoryPath, "stats.json");

    public AppDataStore(string directory, TimeProvider? timeProvider = null)
    {
        DirectoryPath = directory ?? throw new ArgumentNullException(nameof(directory));
        _timeProvider = timeProvider ?? TimeProvider.System;
    }

    public OcySettings LoadSettings()
    {
        if (!Path.Exists(SettingsPath))
        {
            var defaults = new OcySettings();
            defaults.Clamp();
            return defaults;
        }

        try
        {
            var json = File.ReadAllText(SettingsPath);
            var settings = JsonSerializer.Deserialize(json, OcyJsonContext.Default.OcySettings);
            if (settings == null)
            {
                BackupCorruptFile(SettingsPath);
                var fallback = new OcySettings();
                fallback.Clamp();
                return fallback;
            }

            settings.Clamp();
            return settings;
        }
        catch (JsonException)
        {
            BackupCorruptFile(SettingsPath);
            var fallback = new OcySettings();
            fallback.Clamp();
            return fallback;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            var fallback = new OcySettings();
            fallback.Clamp();
            return fallback;
        }
    }

    public void SaveSettings(OcySettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        settings.Clamp();
        var json = JsonSerializer.Serialize(settings, OcyJsonContext.Default.OcySettings);
        AtomicWrite(SettingsPath, json);
    }

    public EngineState LoadState()
    {
        if (!Path.Exists(StatePath))
        {
            return new EngineState();
        }

        try
        {
            var json = File.ReadAllText(StatePath);
            var state = JsonSerializer.Deserialize(json, OcyJsonContext.Default.EngineState);
            if (state == null)
            {
                BackupCorruptFile(StatePath);
                return new EngineState();
            }

            return state;
        }
        catch (JsonException)
        {
            BackupCorruptFile(StatePath);
            return new EngineState();
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return new EngineState();
        }
    }

    public void SaveState(EngineState state)
    {
        ArgumentNullException.ThrowIfNull(state);
        var json = JsonSerializer.Serialize(state, OcyJsonContext.Default.EngineState);
        AtomicWrite(StatePath, json);
    }

    public StatisticsData LoadStats()
    {
        if (!Path.Exists(StatsPath))
        {
            return new StatisticsData();
        }

        try
        {
            var json = File.ReadAllText(StatsPath);
            var stats = JsonSerializer.Deserialize(json, OcyJsonContext.Default.StatisticsData);
            if (stats == null)
            {
                BackupCorruptFile(StatsPath);
                return new StatisticsData();
            }

            return stats;
        }
        catch (JsonException)
        {
            BackupCorruptFile(StatsPath);
            return new StatisticsData();
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return new StatisticsData();
        }
    }

    public void SaveStats(StatisticsData stats)
    {
        ArgumentNullException.ThrowIfNull(stats);
        var json = JsonSerializer.Serialize(stats, OcyJsonContext.Default.StatisticsData);
        AtomicWrite(StatsPath, json);
    }

    private void AtomicWrite(string targetPath, string content)
    {
        Directory.CreateDirectory(DirectoryPath);
        var tmpPath = targetPath + ".tmp";
        File.WriteAllText(tmpPath, content);
        File.Move(tmpPath, targetPath, overwrite: true);
    }

    private void BackupCorruptFile(string filePath)
    {
        var timestamp = _timeProvider.GetUtcNow().ToString("yyyyMMddTHHmmssZ", System.Globalization.CultureInfo.InvariantCulture);
        var backupPath = $"{filePath}.bak-{timestamp}";
        int counter = 1;
        while (File.Exists(backupPath))
        {
            backupPath = $"{filePath}.bak-{timestamp}-{counter++}";
        }

        try
        {
            File.Move(filePath, backupPath, overwrite: true);
        }
        catch
        {
            // Best effort corrupt backup
        }
    }
}
