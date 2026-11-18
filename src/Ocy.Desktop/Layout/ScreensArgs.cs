namespace Ocy.Desktop.Layout;

public static class ScreensArgs
{
    public static bool IsRequested(string[] args)
    {
        ArgumentNullException.ThrowIfNull(args);
        foreach (var arg in args)
        {
            if (string.Equals(arg, "--screens", StringComparison.OrdinalIgnoreCase) ||
                arg.StartsWith("--screens=", StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
        }

        return false;
    }

    public static string? ParseOutFile(string[] args)
    {
        ArgumentNullException.ThrowIfNull(args);
        for (var i = 0; i < args.Length; i++)
        {
            if (string.Equals(args[i], "--screens", StringComparison.OrdinalIgnoreCase))
            {
                if (i + 1 < args.Length && !args[i + 1].StartsWith('-'))
                {
                    return args[i + 1];
                }

                return null;
            }

            if (args[i].StartsWith("--screens=", StringComparison.OrdinalIgnoreCase))
            {
                return args[i]["--screens=".Length..].Trim('"');
            }
        }

        return null;
    }
}
