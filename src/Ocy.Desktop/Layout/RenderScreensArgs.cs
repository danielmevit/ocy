namespace Ocy.Desktop.Layout;

public static class RenderScreensArgs
{
    public static bool IsRequested(string[] args)
    {
        ArgumentNullException.ThrowIfNull(args);
        foreach (var arg in args)
        {
            if (string.Equals(arg, "--render-screens", StringComparison.OrdinalIgnoreCase) ||
                arg.StartsWith("--render-screens=", StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
        }

        return false;
    }

    public static string? ParseDirectory(string[] args)
    {
        ArgumentNullException.ThrowIfNull(args);
        for (var i = 0; i < args.Length; i++)
        {
            if (string.Equals(args[i], "--render-screens", StringComparison.OrdinalIgnoreCase))
            {
                if (i + 1 < args.Length && !args[i + 1].StartsWith('-'))
                {
                    return args[i + 1].Trim('"');
                }

                return null;
            }

            if (args[i].StartsWith("--render-screens=", StringComparison.OrdinalIgnoreCase))
            {
                return args[i]["--render-screens=".Length..].Trim('"');
            }
        }

        return null;
    }
}
