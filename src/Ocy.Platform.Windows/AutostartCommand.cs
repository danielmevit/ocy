namespace Ocy.Platform.Windows;

public static class AutostartCommand
{
    public static string Build(string exePath)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(exePath);
        if (exePath.Contains('"'))
        {
            throw new ArgumentException("The path must not contain quote characters.", nameof(exePath));
        }

        return "\"" + exePath + "\" --autostart";
    }
}
