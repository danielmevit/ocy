using System.Diagnostics;
using System.Globalization;

namespace Ocy.Desktop.Host;

public static class ErrorLogger
{
    public const int MaxLogSizeBytes = 256 * 1024;
    private static readonly object _gate = new();

    public static void Log(Exception? ex, string? context = null)
    {
        Log(null, context, ex);
    }

    public static void Log(string? dataDir, string? context, Exception? ex)
    {
        try
        {
            var message = FormatMessage(context, ex);
            Trace.TraceError(message);

            var dir = dataDir ?? AppDataPaths.GetDataDirectory();
            WriteToFile(dir, message);
        }
        catch
        {
            // Never throw from logger
        }
    }

    private static string FormatMessage(string? context, Exception? ex)
    {
        var timestamp = DateTimeOffset.UtcNow.ToString("yyyy-MM-ddTHH:mm:ss.fffZ", CultureInfo.InvariantCulture);
        if (string.IsNullOrWhiteSpace(context))
        {
            return $"[{timestamp}] {ex}";
        }

        return $"[{timestamp}] {context}: {ex}";
    }

    private static void WriteToFile(string dataDir, string message)
    {
        lock (_gate)
        {
            try
            {
                Directory.CreateDirectory(dataDir);
                var logFile = Path.Combine(dataDir, "ocy-errors.log");
                var oldLogFile = Path.Combine(dataDir, "ocy-errors.old.log");

                if (File.Exists(logFile))
                {
                    var fileInfo = new FileInfo(logFile);
                    if (fileInfo.Length >= MaxLogSizeBytes)
                    {
                        try
                        {
                            File.Move(logFile, oldLogFile, overwrite: true);
                        }
                        catch
                        {
                            try
                            {
                                if (File.Exists(oldLogFile))
                                {
                                    File.Delete(oldLogFile);
                                }
                                File.Move(logFile, oldLogFile);
                            }
                            catch
                            {
                                // Best effort file rotation
                            }
                        }
                    }
                }

                File.AppendAllText(logFile, message + Environment.NewLine);
            }
            catch
            {
                // Never throw from logger
            }
        }
    }
}
