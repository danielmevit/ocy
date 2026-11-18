namespace Ocy.Desktop.Coordinating;

public sealed class NullSettingsWindow : ISettingsWindow
{
    public static readonly NullSettingsWindow Instance = new();

    public void Show()
    {
    }
}
